use axum::body::Body;
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use laplace_common::api::Peer;
use laplace_common::lapp::settings::GossipsubSettings;
use laplace_wasm::http;
use tower::util::ServiceExt;
use tower_http::services::ServeFile;
use truba::{Context, Sender};

use crate::convert;
use crate::error::{ServerError, ServerResult};
use crate::lapps::{LappsProvider, Permission};
use crate::service::gossipsub::{self, decode_keypair, decode_peer_id, GossipsubService, GossipsubServiceMessage};
use crate::service::lapp::LappServiceMessage;
use crate::service::websocket::{WebSocketService, WsServiceMessage};
use crate::service::Addr;

pub async fn index_file(
    State(lapps_provider): State<LappsProvider>,
    Path(lapp_name): Path<String>,
    request: Request<Body>,
) -> impl IntoResponse {
    lapps_provider
        .handle_client_http(lapp_name, move |lapps_provider, lapp_name| async move {
            let lapp_dir = lapps_provider.read_manager().await.lapp_dir(&lapp_name);
            let index_file = lapp_dir.index_file();

            Ok(ServeFile::new(index_file)
                .oneshot(request)
                .await
                .expect("Infallible call"))
        })
        .await
}

pub async fn static_file(
    State(lapps_provider): State<LappsProvider>,
    Path((lapp_name, file_path)): Path<(String, String)>,
    request: Request<Body>,
) -> impl IntoResponse {
    lapps_provider
        .handle_client_http(lapp_name, move |lapps_provider, lapp_name| async move {
            let manager = lapps_provider.read_manager().await;
            let lapp_dir = manager.lapp_dir(&lapp_name);

            let mut fs_file_path = lapp_dir.static_dir().join(&file_path);
            if !fs_file_path.exists() {
                let additional_dirs = manager
                    .lapp_settings(&lapp_name)?
                    .application
                    .additional_static_dirs
                    .clone();

                for additional_dir in additional_dirs {
                    let additional_file_path = lapp_dir.join(additional_dir).join(&file_path);
                    if additional_file_path.exists() {
                        fs_file_path = additional_file_path;
                        break;
                    }
                }
            }

            Ok(ServeFile::new(fs_file_path)
                .oneshot(request)
                .await
                .expect("Infallible call"))
        })
        .await
}

pub async fn http(
    State(lapps_provider): State<LappsProvider>,
    Path((lapp_name, _tail)): Path<(String, String)>,
    request: Request<Body>,
) -> impl IntoResponse {
    lapps_provider
        .handle_client_http(lapp_name, move |lapps_provider, lapp_name| {
            process_http(lapps_provider, lapp_name, request)
        })
        .await
}

async fn process_http(
    lapps_provider: LappsProvider,
    lapp_name: String,
    request: Request<Body>,
) -> ServerResult<Response<Body>> {
    let request = convert::to_wasm_http_request(request).await?;
    let process_http_fut = lapps_provider.read_manager().await.process_http(lapp_name, request);
    let response: http::Response = process_http_fut.await?;

    Response::builder()
        .status(response.status)
        .body(Body::from(response.body))
        .map_err(Into::into)
}

pub async fn ws_start(
    ws: WebSocketUpgrade,
    State(lapps_provider): State<LappsProvider>,
    Path(lapp_name): Path<String>,
) -> impl IntoResponse {
    lapps_provider
        .handle_ws(lapp_name, move |lapps_provider, lapp_name| async move {
            let manager = lapps_provider.read_manager().await;
            let run_lapp_service_fut = manager.run_lapp_service_if_needed(&lapp_name);
            let ctx = manager.ctx().clone();
            drop(manager);

            let lapp_service_sender = run_lapp_service_fut.await?;
            process_ws_start(ctx, ws, lapp_service_sender, lapp_name).await
        })
        .await
}

async fn process_ws_start(
    ctx: Context<Addr>,
    ws: WebSocketUpgrade,
    lapp_service_sender: Sender<LappServiceMessage>,
    lapp_name: String,
) -> ServerResult<impl IntoResponse> {
    let ws_service_addr = Addr::Lapp(lapp_name);
    let lapp_name = ws_service_addr.as_lapp_name();
    let ws_service_sender = ctx.actor_sender::<WsServiceMessage>(ws_service_addr.clone());

    lapp_service_sender
        .send(LappServiceMessage::NewWebSocket(ws_service_sender))
        .map_err(|err| {
            log::error!("Error occurs when send to lapp service: {err:?}, lapp: {lapp_name}");
            ServerError::LappServiceSendError(lapp_name.into())
        })?;

    Ok(ws.on_upgrade({
        move |web_socket| async move {
            WebSocketService::new(web_socket, lapp_service_sender).run(ctx, ws_service_addr);
        }
    }))
}

pub async fn gossipsub_start(
    State(lapps_provider): State<LappsProvider>,
    Path(lapp_name): Path<String>,
    Json(peer): Json<Peer>,
) -> impl IntoResponse {
    lapps_provider
        .handle_allowed(
            &[Permission::ClientHttp, Permission::Tcp],
            lapp_name,
            move |lapps_provider, lapp_name| async move {
                let manager = lapps_provider.read_manager().await;
                let run_lapp_service_fut = manager.run_lapp_service_if_needed(&lapp_name);
                let gossipsub_settings = manager.lapp_settings(&lapp_name)?.network().gossipsub().clone();
                let ctx = manager.ctx().clone();
                drop(manager);

                let lapp_service_sender = run_lapp_service_fut.await?;
                process_gossipsub_start(ctx, lapp_name, lapp_service_sender, peer, gossipsub_settings)
            },
        )
        .await
}

fn process_gossipsub_start(
    ctx: Context<Addr>,
    lapp_name: String,
    lapp_service_sender: Sender<LappServiceMessage>,
    mut peer: Peer,
    settings: GossipsubSettings,
) -> ServerResult<StatusCode> {
    let peer_id = decode_peer_id(&peer.peer_id)?;
    let keypair = decode_keypair(&mut peer.keypair)?;
    let address = settings.addr.parse().map_err(gossipsub::Error::from)?;
    let dial_ports = settings.dial_ports.clone();

    log::info!("Start Gossipsub of lapp \"{lapp_name}\" for peer {peer_id}");
    let gossipsub_service_addr = Addr::Lapp(lapp_name.clone());
    GossipsubService::run(
        ctx.clone(),
        gossipsub_service_addr.clone(),
        keypair,
        peer_id,
        &[],
        address,
        dial_ports,
        "test-net",
        lapp_service_sender.clone(),
    )
    .map_err(|err| {
        log::error!("Error occurs when run gossipsub service: {err:?}");
        err
    })?;
    let gossipsub_service_sender = ctx.actor_sender::<GossipsubServiceMessage>(gossipsub_service_addr);

    lapp_service_sender
        .send(LappServiceMessage::NewGossipsub(gossipsub_service_sender))
        .map_err(|err| {
            log::error!("Error occurs when send to lapp service: {err:?}");
            ServerError::LappServiceSendError(lapp_name)
        })?;

    Ok(StatusCode::OK)
}

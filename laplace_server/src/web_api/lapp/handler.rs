use axum::body::{Body, Bytes, Full};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::Request;
use axum::response::{IntoResponse, Response};
use axum::Json;
use laplace_common::api::Peer;
use laplace_common::lapp::settings::GossipsubSettings;
use laplace_wasm::http;
use reqwest::StatusCode;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use truba::{Context, Sender};

use crate::convert;
use crate::error::ServerResult;
use crate::lapps::{Lapp, LappsProvider, Permission, SharedLapp};
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
            let manager = lapps_provider.read_manager().await;
            let lapp = manager.lapp(&lapp_name)?;
            let index_file = lapp.read().await.index_file();

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
            let mut fs_file_path = lapp_dir.join(Lapp::static_dir_name()).join(&file_path);

            if !fs_file_path.exists() {
                let additional_dirs = manager
                    .lapp(&lapp_name)?
                    .read()
                    .await
                    .settings()
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
        .handle_client_http_lapp(lapp_name, move |_, lapp| process_http(lapp, request))
        .await
}

async fn process_http(lapp: SharedLapp, request: Request<Body>) -> ServerResult<Response<Full<Bytes>>> {
    let request = convert::to_wasm_http_request(request).await?;
    let response: http::Response = lapp.write().await.process_http(request)?;

    Response::builder()
        .status(response.status)
        .body(Full::from(response.body))
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
            let lapp_service_sender = manager.lapp(&lapp_name)?.run_service_if_needed(manager.ctx());
            process_ws_start(manager.ctx().clone(), ws, lapp_service_sender, lapp_name).await
        })
        .await
}

async fn process_ws_start(
    ctx: Context<Addr>,
    ws: WebSocketUpgrade,
    lapp_service_sender: Sender<LappServiceMessage>,
    lapp_name: String,
) -> ServerResult<impl IntoResponse> {
    let ws_actor_id = Addr::Lapp(lapp_name.clone());
    let ws_service_sender = ctx.actor_sender::<WsServiceMessage>(ws_actor_id.clone());

    lapp_service_sender
        .send(LappServiceMessage::NewWebSocket(ws_service_sender))
        .map_err(|err| log::error!("Error occurs when send to lapp service: {err:?}, lapp: {lapp_name}"))
        .ok();

    Ok(ws.on_upgrade({
        move |web_socket| async move {
            WebSocketService::new(web_socket, lapp_service_sender).run(ctx, ws_actor_id);
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
                let lapp_service_sender = manager.lapp(&lapp_name)?.run_service_if_needed(manager.ctx());

                let lapp = manager.lapp(&lapp_name)?;
                let gossipsub_settings = lapp.read().await.settings().network().gossipsub().clone();
                process_gossipsub_start(
                    manager.ctx().clone(),
                    lapp_name,
                    lapp_service_sender,
                    peer,
                    gossipsub_settings,
                )
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

    let gossipsub_actor_id = Addr::Lapp(lapp_name.clone());
    let gossipsub_service_sender = ctx.actor_sender::<GossipsubServiceMessage>(gossipsub_actor_id.clone());
    log::info!("Start P2P for peer {peer_id}");

    GossipsubService::run(
        ctx,
        gossipsub_actor_id,
        keypair,
        peer_id,
        &[],
        address,
        dial_ports,
        "test-net",
        lapp_service_sender.clone(),
    )?;

    lapp_service_sender
        .send(LappServiceMessage::NewGossipSub(gossipsub_service_sender))
        .map_err(|err| log::error!("Error occurs when send to lapp service: {err:?}"))
        .ok();

    Ok(StatusCode::OK)
}

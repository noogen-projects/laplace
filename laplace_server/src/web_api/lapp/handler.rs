use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws::WsResponseBuilder;
use laplace_common::api::Peer;
use laplace_common::lapp::settings::GossipsubSettings;
use laplace_wasm::http;

use crate::error::ServerResult;
use crate::lapps::{Lapp, LappsProvider, Permission, SharedLapp};
use crate::service::gossipsub::{self, decode_keypair, decode_peer_id, GossipsubService};
use crate::service::websocket::WebSocketService;
use crate::{convert, service};

pub async fn index_file(
    lapps_service: web::Data<LappsProvider>,
    lapp_name: web::Path<String>,
    request: HttpRequest,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_client_http(lapp_name.into_inner(), move |lapps_provider, lapp_name| async move {
            let manager = lapps_provider.read_manager().await;
            let lapp = manager.lapp(&lapp_name)?;
            let index_file = lapp.read().await.index_file();

            Ok(NamedFile::open(index_file)?.into_response(&request))
        })
        .await
}

pub async fn static_file(
    lapps_service: web::Data<LappsProvider>,
    lapp_name: String,
    file_path: String,
    request: HttpRequest,
) -> HttpResponse {
    lapps_service
        .into_inner()
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

            Ok(NamedFile::open(fs_file_path)?.into_response(&request))
        })
        .await
}

pub async fn http(
    lapps_service: web::Data<LappsProvider>,
    lapp_name: String,
    request: HttpRequest,
    body: Option<web::Bytes>,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_client_http_lapp(lapp_name, move |_, lapp| {
            process_http(lapp, request, body.map(|bytes| bytes.to_vec()))
        })
        .await
}

async fn process_http(lapp: SharedLapp, request: HttpRequest, body: Option<Vec<u8>>) -> ServerResult<HttpResponse> {
    let request = convert::to_wasm_http_request(&request, body);
    let response: http::Response = lapp.write().await.process_http(request)?;

    Ok(HttpResponse::build(response.status).body(response.body))
}

pub async fn ws_start(
    lapps_service: web::Data<LappsProvider>,
    lapp_name: web::Path<String>,
    request: HttpRequest,
    stream: web::Payload,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_ws(lapp_name.into_inner(), move |lapps_provider, lapp_name| async move {
            let lapp_service_sender = lapps_provider
                .read_manager()
                .await
                .lapp(&lapp_name)?
                .run_service_if_needed()
                .await?;
            process_ws_start(lapp_service_sender, lapp_name, request, stream).await
        })
        .await
}

async fn process_ws_start(
    lapp_service_sender: service::lapp::Sender,
    lapp_name: String,
    request: HttpRequest,
    stream: web::Payload,
) -> ServerResult<HttpResponse> {
    let (addr, response) = WsResponseBuilder::new(WebSocketService::new(lapp_service_sender.clone()), &request, stream)
        .start_with_addr()?;

    lapp_service_sender
        .send(service::lapp::Message::NewWebSocket(addr))
        .map_err(|err| log::error!("Error occurs when send to lapp service: {err:?}, lapp: {lapp_name}"))
        .ok();
    Ok(response)
}

pub async fn gossipsub_start(
    lapps_service: web::Data<LappsProvider>,
    lapp_name: web::Path<String>,
    request: web::Json<Peer>,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_allowed(
            &[Permission::ClientHttp, Permission::Tcp],
            lapp_name.into_inner(),
            move |lapps_provider, lapp_name| async move {
                let manager = lapps_provider.read_manager().await;
                let lapp_service_sender = manager.lapp(&lapp_name)?.run_service_if_needed().await?;

                let lapp = manager.lapp(&lapp_name)?;
                let gossipsub_settings = lapp.read().await.settings().network().gossipsub().clone();
                process_gossipsub_start(lapp_service_sender, request, gossipsub_settings).await
            },
        )
        .await
}

async fn process_gossipsub_start(
    lapp_service_sender: service::lapp::Sender,
    mut request: web::Json<Peer>,
    settings: GossipsubSettings,
) -> ServerResult<HttpResponse> {
    let peer_id = decode_peer_id(&request.peer_id)?;
    let keypair = decode_keypair(&mut request.keypair)?;
    let address = settings.addr.parse().map_err(gossipsub::Error::from)?;
    let dial_ports = settings.dial_ports.clone();

    log::info!("Start P2P for peer {peer_id}");
    let (service, sender) = GossipsubService::new(
        keypair,
        peer_id,
        &[],
        address,
        dial_ports,
        "test-net",
        lapp_service_sender.clone(),
    )?;
    actix::spawn(service);

    lapp_service_sender
        .send(service::lapp::Message::NewGossipSub(sender))
        .map_err(|err| log::error!("Error occurs when send to lapp service: {err:?}"))
        .ok();

    Ok(HttpResponse::Ok().finish())
}

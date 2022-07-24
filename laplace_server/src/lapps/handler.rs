use std::convert::TryFrom;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws::WsResponseBuilder;
use borsh::{BorshDeserialize, BorshSerialize};
use futures::{future, FutureExt, TryFutureExt};

use laplace_common::{api::Peer, lapp::settings::GossipsubSettings};
use laplace_wasm::http;

use crate::{
    convert,
    error::ServerResult,
    lapps::{ExpectedInstance, Instance, Lapp, LappsProvider, Permission},
    service,
    service::{
        gossipsub::{self, decode_keypair, decode_peer_id, GossipsubService},
        websocket::WebSocketService,
    },
};

pub async fn index_file(
    lapps_service: web::Data<LappsProvider>,
    lapp_name: web::Path<String>,
    request: HttpRequest,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_client_http(lapp_name.into_inner(), move |lapps_provider, lapp_name| {
            lapps_provider
                .read_manager()
                .and_then(|manager| manager.lapp(&lapp_name).map(|lapp| lapp.index_file()))
                .map(|index| async move { Ok(NamedFile::open(index)?.into_response(&request)) }.left_future())
                .unwrap_or_else(|err| future::ready(Err(err)).right_future())
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
        .handle_client_http(lapp_name, move |lapps_provider, lapp_name| {
            lapps_provider
                .read_manager()
                .map(|manager| {
                    let lapp_dir = manager.lapp_dir(&lapp_name);
                    let mut fs_file_path = lapp_dir.join(Lapp::static_dir_name()).join(&file_path);

                    if !fs_file_path.exists() {
                        let additional_dirs = match manager
                            .lapp(&lapp_name)
                            .map(|lapp| lapp.settings().application.additional_static_dirs.clone())
                        {
                            Ok(dirs) => dirs,
                            Err(err) => return future::ready(Err(err)).right_future(),
                        };

                        for additional_dir in additional_dirs {
                            let additional_file_path = lapp_dir.join(additional_dir).join(&file_path);
                            if additional_file_path.exists() {
                                fs_file_path = additional_file_path;
                                break;
                            }
                        }
                    }

                    async move { Ok(NamedFile::open(fs_file_path)?.into_response(&request)) }.left_future()
                })
                .unwrap_or_else(|err| future::ready(Err(err)).right_future())
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
        .handle_client_http_lapp(lapp_name, move |_, _, lapp_instance| {
            process_http(lapp_instance, request, body.map(|bytes| bytes.to_vec()))
        })
        .await
}

async fn process_http(
    lapp_instance: Instance,
    request: HttpRequest,
    body: Option<Vec<u8>>,
) -> ServerResult<HttpResponse> {
    let instance = ExpectedInstance::try_from(lapp_instance)?;
    let process_http_fn = instance.exports.get_function("process_http")?.native::<u64, u64>()?;

    let request = convert::to_wasm_http_request(&request, body);
    let bytes = request.try_to_vec()?;
    let arg = instance.bytes_to_wasm_slice(&bytes)?;

    let slice = web::block(move || process_http_fn.call(arg.into())).await??;
    let bytes = unsafe { instance.wasm_slice_to_vec(slice)? };
    let response: http::Response = BorshDeserialize::deserialize(&mut bytes.as_slice())?;

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
        .handle_ws(lapp_name.into_inner(), move |lapps_provider, lapp_name| {
            lapps_provider
                .read_manager()
                .and_then(|manager| {
                    manager
                        .lapp_mut(&lapp_name)
                        .and_then(|mut lapp| lapp.run_service_if_needed())
                })
                .map(|lapp_service_sender| {
                    process_ws_start(lapp_service_sender, lapp_name, request, stream).left_future()
                })
                .unwrap_or_else(|err| future::ready(Err(err)).right_future())
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
        .map_err(|err| log::error!("Error occurs when send to lapp service: {:?}, lapp: {}", err, lapp_name))
        .await
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
            move |lapps_provider, lapp_name| {
                lapps_provider
                    .read_manager()
                    .and_then(|manager| {
                        manager
                            .lapp_mut(&lapp_name)
                            .and_then(|mut lapp| lapp.run_service_if_needed())
                            .and_then(|lapp_service_sender| {
                                manager.lapp(&lapp_name).map(|lapp| {
                                    future::Either::Left(process_gossipsub_start(
                                        lapp_service_sender,
                                        request,
                                        lapp.settings().network().gossipsub().clone(),
                                    ))
                                })
                            })
                    })
                    .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
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

    log::info!("Start P2P for peer {}", peer_id);
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
        .map_err(|err| log::error!("Error occurs when send to lapp service: {:?}", err))
        .await
        .ok();

    Ok(HttpResponse::Ok().finish())
}

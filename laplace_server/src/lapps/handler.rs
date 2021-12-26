use std::convert::TryFrom;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use borsh::{BorshDeserialize, BorshSerialize};
use futures::{future, TryFutureExt};
use laplace_common::{api::Peer, lapp::settings::GossipsubSettings};
use laplace_wasm::http;

use crate::{
    convert,
    error::ServerResult,
    gossipsub::{self, decode_keypair, decode_peer_id, GossipsubService},
    lapps::{service, ExpectedInstance, Instance, LappsProvider, Permission},
    ws::WebSocketService,
};

pub async fn index_file(
    lapps_service: web::Data<LappsProvider>,
    request: HttpRequest,
    lapp_name: String,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_client_http(lapp_name, move |lapps_manager, lapp_name| {
            lapps_manager
                .lapp(&lapp_name)
                .map(|lapp| {
                    let index = lapp.index_file();
                    future::Either::Left(async move { Ok(NamedFile::open(index)?.into_response(&request)) })
                })
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
}

async fn handle_http(
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

pub async fn get(lapps_service: web::Data<LappsProvider>, request: HttpRequest, lapp_name: String) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_client_http_lapp(lapp_name, move |_, _, lapp_instance| {
            handle_http(lapp_instance, request, None)
        })
        .await
}

pub async fn post(
    lapps_service: web::Data<LappsProvider>,
    request: HttpRequest,
    body: web::Bytes,
    lapp_name: String,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_client_http_lapp(lapp_name, move |_, _, lapp_instance| {
            handle_http(lapp_instance, request, Some(body.to_vec()))
        })
        .await
}

pub async fn ws_start(
    lapps_service: web::Data<LappsProvider>,
    request: HttpRequest,
    stream: web::Payload,
    lapp_name: String,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_ws(lapp_name, move |lapps_manager, lapp_name| {
            lapps_manager
                .service_sender(&lapp_name)
                .map(|lapp_service_sender| {
                    future::Either::Left(ws_start_handler(lapp_service_sender, lapp_name, request, stream))
                })
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
}

async fn ws_start_handler(
    lapp_service_sender: service::Sender,
    lapp_name: String,
    request: HttpRequest,
    stream: web::Payload,
) -> ServerResult<HttpResponse> {
    let (addr, response) = ws::start_with_addr(WebSocketService::new(lapp_service_sender.clone()), &request, stream)?;

    lapp_service_sender
        .send(service::Message::NewWebSocket(addr))
        .map_err(|err| log::error!("Error occurs when send to lapp service: {:?}, lapp: {}", err, lapp_name))
        .await
        .ok();
    Ok(response)
}

pub async fn gossipsub_start(
    lapps_service: web::Data<LappsProvider>,
    request: web::Json<Peer>,
    lapp_name: String,
) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle_allowed(
            &[Permission::ClientHttp, Permission::Tcp],
            lapp_name,
            move |lapps_manager, lapp_name| {
                lapps_manager
                    .service_sender(&lapp_name)
                    .and_then(|lapp_service_sender| {
                        lapps_manager.lapp(&lapp_name).map(|lapp| {
                            future::Either::Left(gossipsub_start_handler(
                                lapp_service_sender,
                                request,
                                lapp.settings().network().gossipsub().clone(),
                            ))
                        })
                    })
                    .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
            },
        )
        .await
}

async fn gossipsub_start_handler(
    lapp_service_sender: service::Sender,
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
        .send(service::Message::NewGossipSub(sender))
        .map_err(|err| log::error!("Error occurs when send to lapp service: {:?}", err))
        .await
        .ok();

    Ok(HttpResponse::Ok().finish())
}

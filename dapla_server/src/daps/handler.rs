use std::convert::TryFrom;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use borsh::{BorshDeserialize, BorshSerialize};
use dapla_common::{api::Peer, dap::settings::GossipsubSettings};
use dapla_wasm::http;
use futures::{future, TryFutureExt};

use crate::{
    convert,
    daps::{service, DapsProvider, ExpectedInstance, Instance, Permission},
    error::ServerResult,
    gossipsub::{self, decode_keypair, decode_peer_id, GossipsubService},
    ws::WebSocketService,
};

pub async fn index_file(daps_service: web::Data<DapsProvider>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_client_http(dap_name, move |daps_manager, dap_name| {
            daps_manager
                .dap(&dap_name)
                .map(|dap| {
                    let index = dap.index_file();
                    future::Either::Left(async move { Ok(NamedFile::open(index)?.into_response(&request)) })
                })
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
}

async fn handle_http(
    dap_instance: Instance,
    request: HttpRequest,
    body: Option<Vec<u8>>,
) -> ServerResult<HttpResponse> {
    let instance = ExpectedInstance::try_from(dap_instance)?;
    let process_http_fn = instance.exports.get_function("process_http")?.native::<u64, u64>()?;

    let request = convert::to_wasm_http_request(&request, body);
    let bytes = request.try_to_vec()?;
    let arg = instance.bytes_to_wasm_slice(&bytes)?;

    let slice = web::block(move || process_http_fn.call(arg.into())).await??;
    let bytes = unsafe { instance.wasm_slice_to_vec(slice)? };
    let response: http::Response = BorshDeserialize::deserialize(&mut bytes.as_slice())?;

    Ok(HttpResponse::build(response.status).body(response.body))
}

pub async fn get(daps_service: web::Data<DapsProvider>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_client_http_dap(dap_name, move |_, _, dap_instance| {
            handle_http(dap_instance, request, None)
        })
        .await
}

pub async fn post(
    daps_service: web::Data<DapsProvider>,
    request: HttpRequest,
    body: web::Bytes,
    dap_name: String,
) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_client_http_dap(dap_name, move |_, _, dap_instance| {
            handle_http(dap_instance, request, Some(body.to_vec()))
        })
        .await
}

pub async fn ws_start(
    daps_service: web::Data<DapsProvider>,
    request: HttpRequest,
    stream: web::Payload,
    dap_name: String,
) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_ws(dap_name, move |daps_manager, dap_name| {
            daps_manager
                .service_sender(&dap_name)
                .map(|dap_service_sender| {
                    future::Either::Left(ws_start_handler(dap_service_sender, dap_name, request, stream))
                })
                .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
        })
        .await
}

async fn ws_start_handler(
    dap_service_sender: service::Sender,
    dap_name: String,
    request: HttpRequest,
    stream: web::Payload,
) -> ServerResult<HttpResponse> {
    let (addr, response) = ws::start_with_addr(WebSocketService::new(dap_service_sender.clone()), &request, stream)?;

    dap_service_sender
        .send(service::Message::NewWebSocket(addr))
        .map_err(|err| log::error!("Error occurs when send to dap service: {:?}, dap: {}", err, dap_name))
        .await
        .ok();
    Ok(response)
}

pub async fn gossipsub_start(
    daps_service: web::Data<DapsProvider>,
    request: web::Json<Peer>,
    dap_name: String,
) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_allowed(
            &[Permission::ClientHttp, Permission::Tcp],
            dap_name,
            move |daps_manager, dap_name| {
                daps_manager
                    .service_sender(&dap_name)
                    .and_then(|dap_service_sender| {
                        daps_manager.dap(&dap_name).map(|dap| {
                            future::Either::Left(gossipsub_start_handler(
                                dap_service_sender,
                                request,
                                dap.settings().network.gossipsub.clone(),
                            ))
                        })
                    })
                    .unwrap_or_else(|err| future::Either::Right(future::ready(Err(err))))
            },
        )
        .await
}

async fn gossipsub_start_handler(
    dap_service_sender: service::Sender,
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
        dap_service_sender.clone(),
    )?;
    actix::spawn(service);

    dap_service_sender
        .send(service::Message::NewGossipSub(sender))
        .map_err(|err| log::error!("Error occurs when send to dap service: {:?}", err))
        .await
        .ok();

    Ok(HttpResponse::Ok().finish())
}

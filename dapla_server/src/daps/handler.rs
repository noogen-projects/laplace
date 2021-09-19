use std::convert::TryFrom;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use borsh::{BorshDeserialize, BorshSerialize};
use dapla_common::api::Peer;
use dapla_wasm::process::http;
use futures::{executor, TryFutureExt};

use crate::{
    convert,
    daps::{service, DapsManager, DapsProvider, ExpectedInstance, Permission},
    error::ServerResult,
    gossipsub::{self, decode_keypair, decode_peer_id, GossipsubService},
    ws::WebSocketService,
};

pub async fn index_file(daps_service: web::Data<DapsProvider>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            let dap = daps_manager.dap(&dap_name)?;
            Ok(NamedFile::open(dap.index_file())?.into_response(&request))
        })
        .await
}

fn handle_http(
    daps_manager: &mut DapsManager,
    dap_name: &str,
    request: &HttpRequest,
    body: Option<Vec<u8>>,
) -> ServerResult<HttpResponse> {
    let instance = ExpectedInstance::try_from(daps_manager.instance(dap_name)?)?;
    let process_http_fn = instance.exports.get_function("process_http")?.native::<u64, u64>()?;

    let request = convert::to_wasm_http_request(request, body);
    let bytes = request.try_to_vec()?;
    let arg = instance.bytes_to_wasm_slice(&bytes)?;

    let slice = process_http_fn.call(arg.into())?;
    let bytes = unsafe { instance.wasm_slice_to_vec(slice)? };
    let response: http::Response = BorshDeserialize::deserialize(&mut bytes.as_slice())?;

    Ok(HttpResponse::build(response.status()).body(response.into_parts().1))
}

pub async fn get(daps_service: web::Data<DapsProvider>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            handle_http(daps_manager, &dap_name, &request, None)
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
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            handle_http(daps_manager, &dap_name, &request, Some(body.to_vec()))
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
        .handle_ws_dap(dap_name, move |daps_manager, dap_name| {
            let dap_service_sender = daps_manager.service_sender(&dap_name)?;

            let (addr, response) =
                ws::start_with_addr(WebSocketService::new(dap_service_sender.clone()), &request, stream)?;

            let fut = dap_service_sender
                .send(service::Message::NewWebSocket(addr))
                .map_err(|err| log::error!("Error occurs when send to dap service: {:?}", err));
            executor::block_on(fut).ok(); // todo: use async

            Ok(response)
        })
        .await
}

pub async fn gossipsub_start(
    daps_service: web::Data<DapsProvider>,
    mut request: web::Json<Peer>,
    dap_name: String,
) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_allowed(
            &[Permission::Http, Permission::Tcp],
            dap_name,
            move |daps_manager, dap_name| {
                let dap_service_sender = daps_manager.service_sender(&dap_name)?;
                let peer_id = decode_peer_id(&request.peer_id)?;
                let keypair = decode_keypair(&mut request.keypair)?;
                let settings = daps_manager.dap(&dap_name)?.settings();
                let address = settings
                    .network
                    .gossipsub
                    .addr
                    .parse()
                    .map_err(gossipsub::Error::from)?;
                let dial_ports = settings.network.gossipsub.dial_ports.clone();

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

                let fut = dap_service_sender
                    .send(service::Message::NewGossipSub(sender))
                    .map_err(|err| log::error!("Error occurs when send to dap service: {:?}", err));
                executor::block_on(fut).ok(); // todo: use async

                Ok(HttpResponse::Ok().finish())
            },
        )
        .await
}

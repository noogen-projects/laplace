use std::convert::TryFrom;

use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};

use crate::daps::{DapsService, ExpectedInstance};

pub async fn index_file(daps_service: web::Data<DapsService>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            let dap = daps_manager.dap(&dap_name)?;
            Ok(NamedFile::open(dap.index_file())?.into_response(&request))
        })
        .await
}

pub async fn get(daps_service: web::Data<DapsService>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            let instance = ExpectedInstance::try_from(daps_manager.instance(&dap_name)?)?;
            let uri = request.path();
            let get_fn = instance.exports.get_function("get")?.native::<u64, u64>()?;

            let uri_arg = instance.bytes_to_wasm_slice(&uri)?;

            let slice = get_fn.call(uri_arg.into())?;
            let body = unsafe { instance.wasm_slice_to_string(slice)? };

            Ok(HttpResponse::Ok().body(body))
        })
        .await
}

pub async fn post(
    daps_service: web::Data<DapsService>,
    request: HttpRequest,
    body: String,
    dap_name: String,
) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            let instance = ExpectedInstance::try_from(daps_manager.instance(&dap_name)?)?;
            let uri = request.path();
            let post_fn = instance.exports.get_function("post")?.native::<(u64, u64), u64>()?;

            let uri_arg = instance.bytes_to_wasm_slice(&uri)?;
            let body_arg = instance.bytes_to_wasm_slice(&body)?;

            let slice = post_fn.call(uri_arg.into(), body_arg.into())?;
            let body = unsafe { instance.wasm_slice_to_string(slice)? };

            Ok(HttpResponse::Ok().body(body))
        })
        .await
}

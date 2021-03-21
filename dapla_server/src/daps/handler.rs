use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use dapla_wasm::WasmSlice;
use wasmer::{Instance, Memory};

use crate::{
    daps::{DapInstance, DapsService},
    error::{ServerError, ServerResult},
};

pub async fn index_file(daps_service: web::Data<DapsService>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            let dap = daps_manager.dap(&dap_name)?;
            NamedFile::open(dap.index_file())?
                .into_response(&request)
                .map_err(Into::into)
        })
        .await
}

pub async fn get(daps_service: web::Data<DapsService>, request: HttpRequest, dap_name: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle_http_dap(dap_name, move |daps_manager, dap_name| {
            let instance = daps_manager.instance(&dap_name)?;
            let uri = request.path();
            let memory = instance.exports.get_memory("memory")?;
            let get_fn = instance.exports.get_function("get")?.native::<u64, u64>()?;

            let uri_arg = unsafe {
                let uri_offset = instance.copy_to(&memory, uri.as_ptr(), uri.len())?;
                WasmSlice::from((uri_offset, uri.len() as _))
            };

            let response = WasmSlice::from(get_fn.call(uri_arg.into())?);
            to_string_response(response, &instance, &memory)
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
            let instance = daps_manager.instance(&dap_name)?;
            let uri = request.path();
            let memory = instance.exports.get_memory("memory")?;
            let post_fn = instance.exports.get_function("post")?.native::<(u64, u64), u64>()?;

            let uri_arg = unsafe {
                let uri_offset = instance.copy_to(&memory, uri.as_ptr(), uri.len())?;
                WasmSlice::from((uri_offset, uri.len() as _))
            };
            let body_arg = unsafe {
                let body_offset = instance.copy_to(&memory, body.as_ptr(), body.len())?;
                WasmSlice::from((body_offset, body.len() as _))
            };

            let response = WasmSlice::from(post_fn.call(uri_arg.into(), body_arg.into())?);
            to_string_response(response, &instance, &memory)
        })
        .await
}

fn to_string_response(response: WasmSlice, instance: &Instance, memory: &Memory) -> ServerResult<HttpResponse> {
    if response.len() as u64 <= memory.data_size() - response.ptr() as u64 {
        let data = unsafe { instance.move_from(memory, response.ptr(), response.len() as _)? };

        String::from_utf8(data)
            .map_err(|_| ServerError::ResultNotParsed)
            .map(|response| HttpResponse::Ok().body(response))
    } else {
        log::error!(
            "Response ptr = {}, len = {}, but memory data size = {}",
            response.ptr(),
            response.len(),
            memory.data_size()
        );
        Err(ServerError::WrongResultLength)
    }
}

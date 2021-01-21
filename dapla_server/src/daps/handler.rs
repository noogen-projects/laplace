use actix_files::NamedFile;
use actix_web::{web, HttpRequest, HttpResponse};
use dapla_wasm::WasmSlice;
use wasmer::{Array, WasmPtr};

use crate::{daps::DapsService, error::ServerError};

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
            for (byte, cell) in uri.bytes().zip(memory.view::<u8>()[0..uri.len()].iter()) {
                cell.set(byte);
            }

            let fn_get = instance.exports.get_function("get")?.native::<(u32, u32), u64>()?;
            let result = WasmSlice::from(fn_get.call(0, uri.len() as _)?);

            WasmPtr::<u8, Array>::new(result.ptr())
                .get_utf8_string(memory, result.len())
                .ok_or(ServerError::ResultNotParsed)
                .map(|response| HttpResponse::Ok().body(response))
        })
        .await
}

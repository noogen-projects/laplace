use actix_web::{web, HttpRequest, HttpResponse};
use dapla_wasm::WasmSlice;
use log::error;
use wasmer::{Array, WasmPtr};

use crate::{
    daps::DapsService,
    error::{ServerError, ServerResult},
};

pub async fn get(daps_service: web::Data<DapsService>, request: HttpRequest, dap_name: String) -> HttpResponse {
    process_get(daps_service, request, dap_name).unwrap_or_else(|err| err.into_http_response())
}

fn process_get(
    daps_service: web::Data<DapsService>,
    request: HttpRequest,
    dap_name: String,
) -> ServerResult<HttpResponse> {
    let daps_manager = daps_service.lock().map_err(|err| {
        error!("Daps service lock should be asquired: {:?}", err);
        ServerError::DapsServiceNotLock
    })?;

    if let Some(instance) = daps_manager.instance(&dap_name) {
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
    } else {
        error!("Dap '{}' is not loaded", dap_name);
        Err(ServerError::DapNotLoaded(dap_name))
    }
}

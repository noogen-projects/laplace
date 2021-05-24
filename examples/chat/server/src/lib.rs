use chat_common::{WsMessage, WsResponse};
use dapla_wasm::WasmSlice;
pub use dapla_wasm::{alloc, dealloc};

#[no_mangle]
pub unsafe extern "C" fn get(uri: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_get(uri.into_string_in_wasm()))
}

#[no_mangle]
pub unsafe extern "C" fn ws_text(msg: WasmSlice) -> WasmSlice {
    let response = do_ws_text(msg.into_string_in_wasm())
        .map(WsResponse::Success)
        .unwrap_or_else(WsResponse::Error);
    WasmSlice::from(serde_json::to_string(&response).unwrap_or_else(WsResponse::make_error_json_string))
}

fn do_get(uri: String) -> String {
    let mut response = String::from("Echo ");
    response.push_str(&uri);
    response
}

fn do_ws_text(msg: String) -> Result<WsMessage, String> {
    let msg: WsMessage = serde_json::from_str(&msg).map_err(|err| err.to_string())?;
    match msg {
        WsMessage::Text { peer_id, msg } => Ok(WsMessage::Text {
            peer_id,
            msg: format!("Echo from WASM: {}", msg),
        }),
        msg => Err(format!("Unexpected message {:?}", msg)),
    }
}

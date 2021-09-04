use borsh::{BorshDeserialize, BorshSerialize};
use chat_common::{WsMessage, WsResponse};
pub use dapla_wasm::{alloc, dealloc};
use dapla_wasm::{
    route::{gossipsub, websocket},
    Route, WasmSlice,
};

#[no_mangle]
pub unsafe extern "C" fn get(uri: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_get(uri.into_string_in_wasm()))
}

#[no_mangle]
pub unsafe extern "C" fn route_ws(msg: WasmSlice) -> WasmSlice {
    let response = do_ws(msg.into_vec_in_wasm())
        .map(WsResponse::Success)
        .unwrap_or_else(WsResponse::Error);

    let routes = match response {
        WsResponse::Success(WsMessage::Text { peer_id, msg }) => {
            vec![Route::GossipSub(gossipsub::Message::Text { peer_id, msg })]
        },
        WsResponse::Success(WsMessage::AddPeer(peer_id)) => vec![Route::GossipSub(gossipsub::Message::Dial(peer_id))],
        WsResponse::Success(WsMessage::AddAddress(address)) => {
            vec![Route::GossipSub(gossipsub::Message::AddAddress(address))]
        },
        response => {
            let message = serde_json::to_string(&response).unwrap_or_else(WsResponse::make_error_json_string);
            vec![Route::WebSocket(websocket::Message::Text(message))]
        },
    };
    WasmSlice::from(routes.try_to_vec().expect("Routes should be serializable"))
}

#[no_mangle]
pub unsafe extern "C" fn route_gossipsub(msg: WasmSlice) -> WasmSlice {
    let response = do_gossipsub(msg.into_vec_in_wasm())
        .map(WsResponse::Success)
        .unwrap_or_else(WsResponse::Error);
    let message = serde_json::to_string(&response).unwrap_or_else(WsResponse::make_error_json_string);
    let routes = vec![Route::WebSocket(websocket::Message::Text(message))];
    WasmSlice::from(routes.try_to_vec().expect("Routes should be serializable"))
}

fn do_get(uri: String) -> String {
    let mut response = String::from("Echo ");
    response.push_str(&uri);
    response
}

fn do_ws(msg: Vec<u8>) -> Result<WsMessage, String> {
    let msg: websocket::Message = BorshDeserialize::deserialize(&mut msg.as_slice()).map_err(|err| err.to_string())?;
    let text = match msg {
        websocket::Message::Text(text) => text,
    };
    let msg: WsMessage = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    match msg {
        WsMessage::AddPeer(peer_id) => Ok(WsMessage::AddPeer(peer_id)),
        WsMessage::AddAddress(address) => Ok(WsMessage::AddAddress(address)),
        WsMessage::Text { peer_id, msg } => Ok(WsMessage::Text { peer_id, msg }),
        msg => Err(format!("Unexpected message {:?}", msg)),
    }
}

fn do_gossipsub(msg: Vec<u8>) -> Result<WsMessage, String> {
    let msg: gossipsub::Message = BorshDeserialize::deserialize(&mut msg.as_slice()).map_err(|err| err.to_string())?;
    match msg {
        gossipsub::Message::Text { peer_id, msg } => Ok(WsMessage::Text { peer_id, msg }),
        gossipsub::Message::Dial(peer_id) => Ok(WsMessage::AddPeer(peer_id)),
        gossipsub::Message::AddAddress(address) => Ok(WsMessage::AddAddress(address)),
    }
}

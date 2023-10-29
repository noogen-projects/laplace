use borsh::BorshDeserialize;
use chat_common::{WsMessage, WsResponse};
use laplace_wasm::route::{gossipsub, websocket};
pub use laplace_wasm::{alloc, dealloc};
use laplace_wasm::{Route, WasmSlice};

#[no_mangle]
pub extern "C" fn route_ws(msg: WasmSlice) -> WasmSlice {
    let response = do_ws(unsafe { msg.into_vec_in_wasm() })
        .map(WsResponse::Success)
        .unwrap_or_else(WsResponse::Error);

    let routes = match response {
        WsResponse::Success(WsMessage::Text { peer_id, msg }) => {
            vec![Route::Gossipsub(gossipsub::MessageOut {
                id: "".to_string(),
                msg: gossipsub::Message::Text { peer_id, msg },
            })]
        },
        WsResponse::Success(WsMessage::AddPeer(peer_id)) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: "".to_string(),
            msg: gossipsub::Message::Dial(peer_id),
        })],
        WsResponse::Success(WsMessage::AddAddress(address)) => {
            vec![Route::Gossipsub(gossipsub::MessageOut {
                id: "".to_string(),
                msg: gossipsub::Message::AddAddress(address),
            })]
        },
        response => {
            let message = serde_json::to_string(&response).unwrap_or_else(WsResponse::make_error_json_string);
            vec![Route::Websocket(websocket::Message::Text(message))]
        },
    };
    WasmSlice::from(borsh::to_vec(&routes).expect("Routes should be serializable"))
}

#[no_mangle]
pub extern "C" fn route_gossipsub(msg: WasmSlice) -> WasmSlice {
    let response = do_gossipsub(unsafe { msg.into_vec_in_wasm() })
        .map(WsResponse::Success)
        .unwrap_or_else(WsResponse::Error);
    let message = serde_json::to_string(&response).unwrap_or_else(WsResponse::make_error_json_string);
    let routes = vec![Route::Websocket(websocket::Message::Text(message))];
    WasmSlice::from(borsh::to_vec(&routes).expect("Routes should be serializable"))
}

fn do_ws(msg: Vec<u8>) -> Result<WsMessage, String> {
    let msg: websocket::Message = BorshDeserialize::deserialize(&mut msg.as_slice()).map_err(|err| err.to_string())?;
    let websocket::Message::Text(text) = msg;
    let msg: WsMessage = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    match msg {
        WsMessage::AddPeer(peer_id) => Ok(WsMessage::AddPeer(peer_id)),
        WsMessage::AddAddress(address) => Ok(WsMessage::AddAddress(address)),
        WsMessage::Text { peer_id, msg } => Ok(WsMessage::Text { peer_id, msg }),
        msg => Err(format!("Unexpected message {:?}", msg)),
    }
}

fn do_gossipsub(msg: Vec<u8>) -> Result<WsMessage, String> {
    let msg: gossipsub::MessageIn =
        BorshDeserialize::deserialize(&mut msg.as_slice()).map_err(|err| err.to_string())?;
    match msg {
        gossipsub::MessageIn::Text { peer_id, msg } => Ok(WsMessage::Text { peer_id, msg }),
        gossipsub::MessageIn::Response { id: _, result: Ok(()) } => Ok(WsMessage::Empty),
        gossipsub::MessageIn::Response {
            id: _,
            result: Err(err),
        } => Err(err.message),
    }
}

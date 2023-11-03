use borsh::BorshDeserialize;
use chat_common::{ChatWsMessage, ChatWsResponse};
use laplace_wasm::route::{gossipsub, websocket};
pub use laplace_wasm::{alloc, dealloc};
use laplace_wasm::{Route, WasmSlice};

#[no_mangle]
pub extern "C" fn route_ws(msg: WasmSlice) -> WasmSlice {
    let routes = match do_ws(unsafe { msg.into_vec_in_wasm() }) {
        DoWsResult::Empty => vec![],
        DoWsResult::Close => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: Default::default(),
            msg: gossipsub::Message::Close,
        })],
        DoWsResult::Msg(ChatWsMessage::Text { peer_id, msg }) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: Default::default(),
            msg: gossipsub::Message::Text { peer_id, msg },
        })],
        DoWsResult::Msg(ChatWsMessage::AddPeer(peer_id)) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: Default::default(),
            msg: gossipsub::Message::Dial(peer_id),
        })],
        DoWsResult::Msg(ChatWsMessage::AddAddress(address)) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: Default::default(),
            msg: gossipsub::Message::AddAddress(address),
        })],
        DoWsResult::Msg(msg) => vec![route_ws_message_out(&ChatWsResponse::Success(msg))],
        DoWsResult::Err(err) => vec![route_ws_message_out(&ChatWsResponse::Error(err))],
    };
    WasmSlice::from(borsh::to_vec(&routes).expect("Routes should be serializable"))
}

fn route_ws_message_out(response: &ChatWsResponse) -> Route {
    let message = serde_json::to_string(response).unwrap_or_else(ChatWsResponse::make_error_json_string);
    Route::WebSocket(websocket::MessageOut {
        id: "".to_string(),
        msg: websocket::Message::Text(message),
    })
}

#[no_mangle]
pub extern "C" fn route_gossipsub(msg: WasmSlice) -> WasmSlice {
    let response = match do_gossipsub(unsafe { msg.into_vec_in_wasm() }) {
        Ok(None) => None,
        Ok(Some(response)) => Some(ChatWsResponse::Success(response)),
        Err(err) => Some(ChatWsResponse::Error(err)),
    };

    let routes = if let Some(response) = response {
        let message = serde_json::to_string(&response).unwrap_or_else(ChatWsResponse::make_error_json_string);
        vec![Route::WebSocket(websocket::MessageOut {
            id: "".to_string(),
            msg: websocket::Message::Text(message),
        })]
    } else {
        vec![]
    };

    WasmSlice::from(borsh::to_vec(&routes).expect("Routes should be serializable"))
}

enum DoWsResult {
    Empty,
    Close,
    Msg(ChatWsMessage),
    Err(String),
}

fn do_ws(msg: Vec<u8>) -> DoWsResult {
    let msg: websocket::MessageIn = match BorshDeserialize::deserialize(&mut msg.as_slice()) {
        Ok(msg) => msg,
        Err(err) => return DoWsResult::Err(err.to_string()),
    };
    match msg {
        websocket::MessageIn::Message(websocket::Message::Text(text)) => {
            let msg: ChatWsMessage = match serde_json::from_str(&text) {
                Ok(msg) => msg,
                Err(err) => return DoWsResult::Err(err.to_string()),
            };
            match msg {
                ChatWsMessage::AddPeer(peer_id) => DoWsResult::Msg(ChatWsMessage::AddPeer(peer_id)),
                ChatWsMessage::AddAddress(address) => DoWsResult::Msg(ChatWsMessage::AddAddress(address)),
                ChatWsMessage::Text { peer_id, msg } => DoWsResult::Msg(ChatWsMessage::Text { peer_id, msg }),
                msg => DoWsResult::Err(format!("Unexpected message {:?}", msg)),
            }
        },
        websocket::MessageIn::Message(websocket::Message::Binary(data)) => {
            DoWsResult::Err(format!("Wrong message data: {data:?}"))
        },
        websocket::MessageIn::Message(websocket::Message::Close) => DoWsResult::Close,
        websocket::MessageIn::Response { id: _, result } => match result {
            Ok(_) => DoWsResult::Empty,
            Err(err) => DoWsResult::Err(err),
        },
        websocket::MessageIn::Timeout => DoWsResult::Err("WebSocket heartbeat timeout".to_string()),
        websocket::MessageIn::Error(err) => DoWsResult::Err(err),
    }
}

fn do_gossipsub(msg: Vec<u8>) -> Result<Option<ChatWsMessage>, String> {
    let msg: gossipsub::MessageIn =
        BorshDeserialize::deserialize(&mut msg.as_slice()).map_err(|err| err.to_string())?;
    match msg {
        gossipsub::MessageIn::Text { peer_id, msg } => Ok(Some(ChatWsMessage::Text { peer_id, msg })),
        gossipsub::MessageIn::Response { id: _, result } => result.map(|_| None).map_err(|err| err.message),
    }
}

use borsh::BorshDeserialize;
use chat_common::{ChatWsMessage, ChatWsRequest, ChatWsResponse};
use laplace_wasm::route::{gossipsub, websocket};
pub use laplace_wasm::{alloc, dealloc};
use laplace_wasm::{Route, WasmSlice};

#[no_mangle]
pub extern "C" fn route_ws(msg: WasmSlice) -> WasmSlice {
    let routes = match do_ws(unsafe { msg.into_vec_in_wasm() }) {
        DoWsResult::Empty => vec![],
        DoWsResult::Close => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: "close".into(),
            msg: gossipsub::Message::Close,
        })],
        DoWsResult::AddPeer(peer_id) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: format!("add_peer:{peer_id}"),
            msg: gossipsub::Message::Dial(peer_id),
        })],
        DoWsResult::AddAddress(address) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: format!("add_address:{address}"),
            msg: gossipsub::Message::AddAddress(address),
        })],
        DoWsResult::Msg(ChatWsMessage { peer_id, msg }) => vec![Route::Gossipsub(gossipsub::MessageOut {
            id: format!("send_message:{peer_id}"),
            msg: gossipsub::Message::Text { peer_id, msg },
        })],
        DoWsResult::Response(response) => vec![route_ws_message_out("", &response)],
    };
    WasmSlice::from(borsh::to_vec(&routes).expect("Routes should be serializable"))
}

#[no_mangle]
pub extern "C" fn route_gossipsub(msg: WasmSlice) -> WasmSlice {
    let response = do_gossipsub(unsafe { msg.into_vec_in_wasm() });
    let routes = vec![route_ws_message_out("", &response)];

    WasmSlice::from(borsh::to_vec(&routes).expect("Routes should be serializable"))
}

fn route_ws_message_out(id: impl Into<String>, response: &ChatWsResponse) -> Route {
    let message = serde_json::to_string(response).unwrap_or_else(ChatWsResponse::make_error_json_string);
    Route::WebSocket(websocket::MessageOut {
        id: id.into(),
        msg: websocket::Message::Text(message),
    })
}

enum DoWsResult {
    Empty,
    Close,
    AddPeer(String),
    AddAddress(String),
    Msg(ChatWsMessage),
    Response(ChatWsResponse),
}

impl From<ChatWsResponse> for DoWsResult {
    fn from(response: ChatWsResponse) -> Self {
        Self::Response(response)
    }
}

fn do_ws(msg: Vec<u8>) -> DoWsResult {
    let msg: websocket::MessageIn = match BorshDeserialize::deserialize(&mut msg.as_slice()) {
        Ok(msg) => msg,
        Err(_err) => return DoWsResult::Close,
    };
    match msg {
        websocket::MessageIn::Message(websocket::Message::Text(text)) => {
            let request: ChatWsRequest = match serde_json::from_str(&text) {
                Ok(request) => request,
                Err(err) => return ChatWsResponse::InternalError(err.to_string()).into(),
            };
            match request {
                ChatWsRequest::AddPeer(peer_id) => DoWsResult::AddPeer(peer_id),
                ChatWsRequest::AddAddress(address) => DoWsResult::AddAddress(address),
                ChatWsRequest::SendMessage(msg) => DoWsResult::Msg(msg),
                request => ChatWsResponse::InternalError(format!("Unexpected request {request:?}")).into(),
            }
        },
        websocket::MessageIn::Message(websocket::Message::Binary(data)) => {
            ChatWsResponse::InternalError(format!("Wrong message data: {data:?}")).into()
        },
        websocket::MessageIn::Response { id: _, result } if result.is_ok() => DoWsResult::Empty,
        _ => DoWsResult::Close,
    }
}

fn do_gossipsub(msg: Vec<u8>) -> ChatWsResponse {
    let msg: gossipsub::MessageIn = match BorshDeserialize::deserialize(&mut msg.as_slice()) {
        Ok(msg) => msg,
        Err(err) => return ChatWsResponse::InternalError(err.to_string()),
    };
    match msg {
        gossipsub::MessageIn::Text { peer_id, msg } => ChatWsResponse::ReceiveMessage(ChatWsMessage { peer_id, msg }),
        gossipsub::MessageIn::Response { id, result } => {
            let result = result.map_err(|err| err.message);
            if let Some(peer_id) = id.strip_prefix("add_peer:") {
                ChatWsResponse::AddPeerResult(peer_id.into(), result)
            } else if let Some(address) = id.strip_prefix("add_address:") {
                ChatWsResponse::AddAddressResult(address.into(), result)
            } else if let Some(peer_id) = id.strip_prefix("send_message:") {
                ChatWsResponse::SendMessageResult(peer_id.into(), result)
            } else {
                ChatWsResponse::InternalError(format!("Unknown operation result. id: {id}, result: {result:?}"))
            }
        },
    }
}

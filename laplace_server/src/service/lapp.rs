use std::io;

use actix::Addr;
use async_std::channel;
use borsh::{BorshDeserialize, BorshSerialize};
use derive_more::From;
use wasmer::NativeFunc;

use laplace_wasm::{Route, WasmSlice};

use crate::{
    lapps::{ExpectedInstance, LappInstanceError},
    service::{gossipsub, websocket},
};

#[derive(Debug, From)]
pub enum Error {
    Export(wasmer::ExportError),
    Runtime(wasmer::RuntimeError),
    Instance(LappInstanceError),
    Io(io::Error),
}

#[derive(Debug)]
pub enum Message {
    Stop,

    // WebSocket
    NewWebSocket(Addr<websocket::WebSocketService>),
    WebSocket(websocket::Message),

    // GossipSub
    NewGossipSub(gossipsub::Sender),
    GossipSub(gossipsub::Message),
}

pub type Sender = channel::Sender<Message>;
pub type Receiver = channel::Receiver<Message>;

#[derive(Clone)]
pub struct LappService {
    instance: ExpectedInstance,
    receiver: Receiver,
    gossipsub_sender: Option<gossipsub::Sender>,
    websocket_sender: Option<Addr<websocket::WebSocketService>>,
}

impl LappService {
    pub fn new(instance: ExpectedInstance) -> (Self, Sender) {
        let (sender, receiver) = channel::unbounded();
        (
            Self {
                instance,
                receiver,
                gossipsub_sender: None,
                websocket_sender: None,
            },
            sender,
        )
    }

    pub fn use_websocket(&mut self, addr: Addr<websocket::WebSocketService>) {
        self.websocket_sender.replace(addr);
    }

    async fn send_websocket(&self, msg: websocket::Message) {
        if let Some(addr) = &self.websocket_sender {
            if let Err(err) = addr.send(websocket::ActixMessage(msg)).await {
                log::error!("Websocket send error: {:?}", err);
            }
        } else {
            log::error!("Uninitialized websocket for msg {:?}", msg);
        }
    }

    fn handle_websocket(&self, msg: &websocket::Message) -> Result<Vec<Route>, Error> {
        let route_ws_fn = self.instance.exports.get_function("route_ws")?.native::<u64, u64>()?;
        let arg = self.instance.bytes_to_wasm_slice(&msg.try_to_vec()?)?;
        self.call_lapp_handler(route_ws_fn, arg)
    }

    pub fn use_gossipsub(&mut self, sender: gossipsub::Sender) {
        self.gossipsub_sender.replace(sender);
    }

    pub fn send_gossipsub(&self, msg: gossipsub::Message) {
        if let Some(sender) = &self.gossipsub_sender {
            if let Err(err) = sender.send(msg) {
                log::error!("Gossipsub send error: {:?}", err);
            }
        } else {
            log::error!("Uninitialized gossipsub for msg {:?}", msg);
        }
    }

    fn handle_gossipsub(&self, msg: &gossipsub::Message) -> Result<Vec<Route>, Error> {
        let route_gossipsub_fn = self
            .instance
            .exports
            .get_function("route_gossipsub")?
            .native::<u64, u64>()?;
        let arg = self.instance.bytes_to_wasm_slice(&msg.try_to_vec()?)?;
        self.call_lapp_handler(route_gossipsub_fn, arg)
    }

    fn call_lapp_handler(&self, handler_fn: NativeFunc<u64, u64>, arg: WasmSlice) -> Result<Vec<Route>, Error> {
        let response_slice = handler_fn.call(arg.into())?;
        let bytes = unsafe { self.instance.wasm_slice_to_vec(response_slice)? };
        let routes = BorshDeserialize::try_from_slice(&bytes)?;

        Ok(routes)
    }

    async fn process_routes(&self, routes: Vec<Route>) {
        log::info!("Routes: {:?}", routes);
        for route in routes {
            match route {
                Route::Http(msg) => log::error!("Unexpected HTTP route: {:?}", msg),
                Route::WebSocket(msg) => self.send_websocket(msg).await,
                Route::GossipSub(msg) => self.send_gossipsub(msg),
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            match self.receiver.recv().await {
                Ok(Message::Stop) => break,

                // WebSocket
                Ok(Message::NewWebSocket(sender)) => self.use_websocket(sender),
                Ok(Message::WebSocket(msg)) => {
                    log::info!("Receive ws message: {:?}", msg);
                    match self.handle_websocket(&msg) {
                        Ok(routes) => self.process_routes(routes).await,
                        Err(err) => log::error!("Handle websocket error: {:?}", err),
                    }
                },

                // GossipSub
                Ok(Message::NewGossipSub(sender)) => self.use_gossipsub(sender),
                Ok(Message::GossipSub(msg)) => match self.handle_gossipsub(&msg) {
                    Ok(routes) => self.process_routes(routes).await,
                    Err(err) => log::error!("Handle gossipsub error: {:?}", err),
                },

                // Error
                Err(err) => log::error!("Receive message error: {:?}", err),
            }
        }
    }
}

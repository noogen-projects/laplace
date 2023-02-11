use std::io;

use actix::Addr;
use derive_more::From;
use laplace_wasm::Route;
use tokio::sync::mpsc;

use crate::lapps::{LappInstanceError, SharedLapp};
use crate::service::{gossipsub, websocket};

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

pub type Sender = mpsc::UnboundedSender<Message>;
pub type Receiver = mpsc::UnboundedReceiver<Message>;

pub struct LappService {
    lapp: SharedLapp,
    receiver: Receiver,
    gossipsub_sender: Option<gossipsub::Sender>,
    websocket_sender: Option<Addr<websocket::WebSocketService>>,
}

impl LappService {
    pub fn new(lapp: SharedLapp) -> (Self, Sender) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                lapp,
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
                log::error!("Websocket send error: {err:?}");
            }
        } else {
            log::error!("Uninitialized websocket for msg {msg:?}");
        }
    }

    pub fn use_gossipsub(&mut self, sender: gossipsub::Sender) {
        self.gossipsub_sender.replace(sender);
    }

    pub fn send_gossipsub(&self, msg: gossipsub::Message) {
        if let Some(sender) = &self.gossipsub_sender {
            if let Err(err) = sender.send(msg) {
                log::error!("Gossipsub send error: {err:?}");
            }
        } else {
            log::error!("Uninitialized gossipsub for msg {msg:?}");
        }
    }

    async fn process_routes(&self, routes: Vec<Route>) {
        log::info!("Routes: {routes:?}");
        for route in routes {
            match route {
                Route::Http(msg) => log::error!("Unexpected HTTP route: {msg:?}"),
                Route::WebSocket(msg) => self.send_websocket(msg).await,
                Route::GossipSub(msg) => self.send_gossipsub(msg),
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            match self.receiver.recv().await {
                None | Some(Message::Stop) => break,

                // WebSocket
                Some(Message::NewWebSocket(sender)) => self.use_websocket(sender),
                Some(Message::WebSocket(msg)) => {
                    log::info!("Receive websocket message: {msg:?}");
                    let mut lapp = self.lapp.write().await;
                    let Some(instance) = lapp.instance_mut() else {
                        log::warn!("Handle websocket: instance not found for lapp {}", lapp.name());
                        continue;
                    };
                    match instance.route_ws(&msg) {
                        Ok(routes) => self.process_routes(routes).await,
                        Err(err) => log::error!("Handle websocket error: {err:?}"),
                    }
                },

                // GossipSub
                Some(Message::NewGossipSub(sender)) => self.use_gossipsub(sender),
                Some(Message::GossipSub(msg)) => {
                    let mut lapp = self.lapp.write().await;
                    let Some(instance) = lapp.instance_mut() else {
                        log::warn!("Handle gossipsub: instance not found for lapp {}", lapp.name());
                        continue;
                    };
                    match instance.route_gossipsub(&msg) {
                        Ok(routes) => self.process_routes(routes).await,
                        Err(err) => log::error!("Handle gossipsub error: {err:?}"),
                    }
                },
            }
        }
    }

    pub async fn stop(sender: Sender) -> bool {
        sender
            .send(Message::Stop)
            .map_err(|err| log::error!("Error occurs when send to lapp service: {err:?}"))
            .is_ok()
    }
}

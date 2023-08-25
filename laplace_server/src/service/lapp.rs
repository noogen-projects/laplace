use std::future::Future;
use std::io;

use derive_more::From;
use laplace_wasm::Route;
use tokio::sync::mpsc;
use truba::{Context, UnboundedMpscChannel};

use crate::lapps::{LappInstanceError, SharedLapp};
use crate::service::{gossipsub, websocket, Addr};

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
    NewWebSocket(actix::Addr<websocket::WebSocketService>),
    WebSocket(websocket::Message),

    // GossipSub
    NewGossipSub(gossipsub::Sender),
    GossipSub(gossipsub::Message),
}

impl truba::Message for Message {
    type Channel = UnboundedMpscChannel<Self>;
}

pub type Sender = mpsc::UnboundedSender<Message>;
pub type Receiver = mpsc::UnboundedReceiver<Message>;

pub struct LappService {
    lapp: SharedLapp,
    gossipsub_sender: Option<gossipsub::Sender>,
    websocket_sender: Option<actix::Addr<websocket::WebSocketService>>,
}

impl LappService {
    pub fn new(lapp: SharedLapp) -> Self {
        Self {
            lapp,
            gossipsub_sender: None,
            websocket_sender: None,
        }
    }

    pub fn run(mut self, ctx: Context<Addr>) {
        let lapp_name = self.lapp.name().to_owned();
        let mut messages_in = ctx.actor_receiver::<Message>(Addr::Lapp(lapp_name));

        truba::spawn_event_loop!(ctx, {
            Some(msg) = messages_in.recv() => {
                match msg {
                    Message::NewWebSocket(sender) => self.handle_new_websocket(sender),
                    Message::WebSocket(msg) => self.handle_websocket(msg).await,

                    Message::NewGossipSub(sender) => self.handle_new_gossipsub(sender),
                    Message::GossipSub(msg) => self.handle_gossipsub(msg).await,

                    Message::Stop => break,
                }
            }
        });
    }

    fn handle_new_websocket(&mut self, addr: actix::Addr<websocket::WebSocketService>) {
        self.websocket_sender.replace(addr);
    }

    async fn handle_websocket(&mut self, msg: websocket::Message) {
        log::info!("Receive websocket message: {msg:?}");
        let mut lapp = self.lapp.write().await;
        let Some(instance) = lapp.instance_mut() else {
            log::warn!("Handle websocket: instance not found for lapp {}", lapp.name());
            return;
        };
        match instance.route_ws(&msg) {
            Ok(routes) => {
                let fut = self.process_routes(routes);
                fut.await
            },
            Err(err) => log::error!("Handle websocket error: {err:?}"),
        }
    }

    fn handle_new_gossipsub(&mut self, sender: gossipsub::Sender) {
        self.gossipsub_sender.replace(sender);
    }

    async fn handle_gossipsub(&mut self, msg: gossipsub::Message) {
        let mut lapp = self.lapp.write().await;
        let Some(instance) = lapp.instance_mut() else {
            log::warn!("Handle gossipsub: instance not found for lapp {}", lapp.name());
            return;
        };
        match instance.route_gossipsub(&msg) {
            Ok(routes) => {
                let fut = self.process_routes(routes);
                fut.await
            },
            Err(err) => log::error!("Handle gossipsub error: {err:?}"),
        }
    }

    fn send_websocket(&self, msg: websocket::Message) -> impl Future<Output = ()> + Send {
        let sender = self.websocket_sender.clone();
        async move {
            if let Some(addr) = sender {
                if let Err(err) = addr.send(websocket::WsServiceMessage(msg)).await {
                    log::error!("Websocket send error: {err:?}");
                }
            } else {
                log::error!("Uninitialized websocket for msg {msg:?}");
            }
        }
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

    fn process_routes(&self, routes: Vec<Route>) -> impl Future<Output = ()> + Send {
        log::info!("Routes: {routes:?}");
        let mut futures = Vec::new();

        for route in routes {
            match route {
                Route::Http(msg) => log::error!("Unexpected HTTP route: {msg:?}"),
                Route::WebSocket(msg) => futures.push(self.send_websocket(msg)),
                Route::GossipSub(msg) => self.send_gossipsub(msg),
            }
        }

        async move {
            futures::future::join_all(futures).await;
        }
    }
}

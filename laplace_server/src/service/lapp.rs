use std::io;

use derive_more::From;
use laplace_wasm::Route;
use truba::{Context, Message, Sender, UnboundedMpscChannel};

use crate::lapps::{LappInstanceError, SharedLapp};
use crate::service::gossipsub::GossipsubServiceMessage;
use crate::service::websocket::{WsMessage, WsServiceMessage};
use crate::service::{gossipsub, Addr};

#[derive(Debug, From)]
pub enum Error {
    Instance(LappInstanceError),
    Io(io::Error),
}

#[derive(Debug)]
pub enum LappServiceMessage {
    Stop,

    // WebSocket
    NewWebSocket(Sender<WsServiceMessage>),
    WebSocket(WsMessage),

    // GossipSub
    NewGossipSub(Sender<GossipsubServiceMessage>),
    GossipSub(GossipsubServiceMessage),
}

impl Message for LappServiceMessage {
    type Channel = UnboundedMpscChannel<Self>;
}

pub struct LappService {
    lapp: SharedLapp,
    gossipsub_sender: Option<Sender<GossipsubServiceMessage>>,
    websocket_sender: Option<Sender<WsServiceMessage>>,
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
        let mut messages_in = ctx.actor_receiver::<LappServiceMessage>(Addr::Lapp(lapp_name));

        log::info!("Run LappService for lapp \"{}\"", self.lapp.name());
        truba::spawn_event_loop!(ctx, {
            Some(msg) = messages_in.recv() => {
                match msg {
                    LappServiceMessage::NewWebSocket(sender) => self.handle_new_websocket(sender),
                    LappServiceMessage::WebSocket(msg) => self.handle_websocket(msg).await,

                    LappServiceMessage::NewGossipSub(sender) => self.handle_new_gossipsub(sender),
                    LappServiceMessage::GossipSub(msg) => self.handle_gossipsub(msg).await,

                    LappServiceMessage::Stop => break,
                }
            }
        });
    }

    fn handle_new_websocket(&mut self, sender: Sender<WsServiceMessage>) {
        self.websocket_sender.replace(sender);
    }

    async fn handle_websocket(&mut self, msg: WsMessage) {
        let mut lapp = self.lapp.write().await;
        let Some(instance) = lapp.instance_mut() else {
            log::warn!("Handle websocket: instance not found for lapp {}", lapp.name());
            return;
        };
        match instance.route_ws(&msg).await {
            Ok(routes) => self.process_routes(routes),
            Err(err) => log::error!("Handle websocket error: {err:?}"),
        }
    }

    fn handle_new_gossipsub(&mut self, sender: Sender<GossipsubServiceMessage>) {
        self.gossipsub_sender.replace(sender);
    }

    async fn handle_gossipsub(&mut self, GossipsubServiceMessage(msg): GossipsubServiceMessage) {
        let mut lapp = self.lapp.write().await;
        let Some(instance) = lapp.instance_mut() else {
            log::warn!("Handle gossipsub: instance not found for lapp {}", lapp.name());
            return;
        };
        match instance.route_gossipsub(&msg).await {
            Ok(routes) => self.process_routes(routes),
            Err(err) => log::error!("Handle gossipsub error: {err:?}"),
        }
    }

    fn send_websocket(&self, msg: WsMessage) {
        let websocket_sender = self.websocket_sender.clone();
        if let Some(sender) = websocket_sender {
            if let Err(err) = sender.send(WsServiceMessage(msg)) {
                log::error!("Websocket send error: {err:?}");
            }
        } else {
            log::error!("Uninitialized websocket for msg {msg:?}");
        }
    }

    pub fn send_gossipsub(&self, msg: gossipsub::Message) {
        if let Some(sender) = &self.gossipsub_sender {
            if let Err(err) = sender.send(GossipsubServiceMessage(msg)) {
                log::error!("Gossipsub send error: {err:?}");
            }
        } else {
            log::error!("Uninitialized gossipsub for msg {msg:?}");
        }
    }

    fn process_routes(&self, routes: Vec<Route>) {
        log::info!("Routes: {routes:?}");

        for route in routes {
            match route {
                Route::Http(msg) => log::error!("Unexpected HTTP route: {msg:?}"),
                Route::WebSocket(msg) => self.send_websocket(msg),
                Route::GossipSub(msg) => self.send_gossipsub(msg),
            }
        }
    }
}

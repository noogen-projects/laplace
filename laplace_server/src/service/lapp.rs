use std::future::Future;
use std::io;

use derive_more::From;
use futures::FutureExt;
use laplace_wasm::http::{Request, Response};
use laplace_wasm::Route;
use reqwest::Client;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use truba::{Context, Message, Sender, UnboundedMpscChannel};

use crate::error::{ServerError, ServerResult};
use crate::lapps::{Lapp, LappInstanceError};
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

    Http(HttpMessage),

    // Websocket
    NewWebsocket(Sender<WsServiceMessage>),
    Websocket(WsMessage),

    // Gossipsub
    NewGossipsub(Sender<GossipsubServiceMessage>),
    Gossipsub(gossipsub::MessageIn),
}

impl Message for LappServiceMessage {
    type Channel = UnboundedMpscChannel<Self>;
}

impl LappServiceMessage {
    pub fn new_http(request: Request) -> (Self, oneshot::Receiver<ServerResult<Response>>) {
        let (response_out, response_in) = oneshot::channel();
        let message = Self::Http(HttpMessage {
            request: Box::new(request),
            response_out,
        });

        (message, response_in)
    }
}

#[derive(Debug)]
pub struct HttpMessage {
    pub request: Box<Request>,
    pub response_out: oneshot::Sender<ServerResult<Response>>,
}

pub struct LappService {
    lapp: Lapp,
    gossipsub_sender: Option<Sender<GossipsubServiceMessage>>,
    websocket_sender: Option<Sender<WsServiceMessage>>,
}

impl LappService {
    pub fn new(lapp: Lapp) -> Self {
        Self {
            lapp,
            gossipsub_sender: None,
            websocket_sender: None,
        }
    }

    pub fn run(mut self, ctx: Context<Addr>, http_client: Client) -> impl Future<Output = ServerResult<()>> {
        let lapp_name = self.lapp.name().to_owned();
        let (instantiate_sender, instantiate_receiver) = oneshot::channel();

        log::info!("Run lapp service for lapp \"{lapp_name}\"");

        let handle = Handle::current();
        std::thread::spawn(move || {
            handle.block_on(async move {
                let mut messages_in = ctx.actor_receiver::<LappServiceMessage>(Addr::Lapp(self.lapp.name().to_owned()));
                let instantiate_result = self.lapp.instantiate(http_client).await;
                let is_instantiated = instantiate_result.is_ok();

                if let Err(instantiate_result) = instantiate_sender.send(instantiate_result) {
                    log::error!("Instantiate receiver dropped, instantiate result: {instantiate_result:?}");
                }

                if is_instantiated {
                    truba::event_loop!(ctx, {
                        Some(msg) = messages_in.recv() => {
                            match msg {
                                LappServiceMessage::Http(msg) => self.handle_http(msg).await,

                                LappServiceMessage::NewWebsocket(sender) => self.handle_new_websocket(sender),
                                LappServiceMessage::Websocket(msg) => self.handle_websocket(msg).await,

                                LappServiceMessage::NewGossipsub(sender) => self.handle_new_gossipsub(sender),
                                LappServiceMessage::Gossipsub(msg) => self.handle_gossipsub(msg).await,

                                LappServiceMessage::Stop => break,
                            }
                        }
                    });
                }
            });
        });

        instantiate_receiver.map(move |result| {
            result
                .map_err(|_| ServerError::LappInitError(format!("Lapp service for lapp \"{lapp_name}\" is dropped")))?
        })
    }

    pub fn is_run(ctx: &Context<Addr>, service_actor_id: &Addr) -> bool {
        ctx.get_actor_sender::<LappServiceMessage>(service_actor_id).is_some()
    }

    pub fn stop(ctx: &Context<Addr>, service_actor_id: &Addr) {
        if let Some(sender) = ctx.get_actor_sender::<LappServiceMessage>(service_actor_id) {
            if let Err(err) = sender.send(LappServiceMessage::Stop) {
                log::error!("Cannot stop lapp service '{service_actor_id}': {err}");
            }
            drop(ctx.extract_actor_channel::<LappServiceMessage>(service_actor_id));
        }
    }

    async fn handle_http(&mut self, msg: HttpMessage) {
        let HttpMessage { request, response_out } = msg;

        let result = self.lapp.process_http(*request).await;
        if let Err(err) = response_out.send(result) {
            log::error!("Cannot process HTTP for lapp '{}': {err:?}", self.lapp.name());
        }
    }

    fn handle_new_websocket(&mut self, sender: Sender<WsServiceMessage>) {
        self.websocket_sender.replace(sender);
    }

    async fn handle_websocket(&mut self, msg: WsMessage) {
        let Some(instance) = self.lapp.instance_mut() else {
            log::warn!("Handle websocket: instance not found for lapp {}", self.lapp.name());
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

    async fn handle_gossipsub(&mut self, msg: gossipsub::MessageIn) {
        let Some(instance) = self.lapp.instance_mut() else {
            log::warn!("Handle gossipsub: instance not found for lapp {}", self.lapp.name());
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

    pub fn send_gossipsub(&self, msg: gossipsub::MessageOut) {
        if let Some(sender) = &self.gossipsub_sender {
            if let Err(err) = sender.send(GossipsubServiceMessage(msg)) {
                log::error!("Gossipsub send error: {err:?}");
            }
        } else {
            log::error!("Uninitialized gossipsub for msg {msg:?}");
        }
    }

    fn process_routes(&self, routes: Vec<Route>) {
        log::debug!("Routes: {routes:?}");

        for route in routes {
            match route {
                Route::Http(msg) => log::error!("Unexpected HTTP route: {msg:?}"),
                Route::Websocket(msg) => self.send_websocket(msg),
                Route::Gossipsub(msg) => self.send_gossipsub(msg),
            }
        }
    }
}

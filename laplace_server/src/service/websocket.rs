use std::io;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket};
use derive_more::From;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
pub use laplace_wasm::route::websocket::Message as WsMessage;
use tokio::time;
use truba::{Context, Sender, UnboundedMpscChannel};

use crate::lapps::LappInstanceError;
use crate::service::lapp::LappServiceMessage;
use crate::service::Addr;

#[derive(Debug, From)]
enum WsError {
    Instance(LappInstanceError),
    Io(io::Error),
}

#[derive(Debug)]
pub struct WsServiceMessage(pub WsMessage);

impl truba::Message for WsServiceMessage {
    type Channel = UnboundedMpscChannel<Self>;
}

#[derive(Debug)]
pub struct WebSocketService {
    /// Client must send ping at least once per SETTINGS.ws.client_timeout_sec seconds,
    /// otherwise we drop connection.
    hb: Instant,

    lapp_service_sender: Sender<LappServiceMessage>,
    ws_sender: SplitSink<WebSocket, Message>,
    ws_receiver: SplitStream<WebSocket>,
}

impl WebSocketService {
    /// How often heartbeat pings are sent
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    /// How long before lack of client response causes a timeout
    const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn new(web_socket: WebSocket, lapp_service_sender: Sender<LappServiceMessage>) -> Self {
        let (ws_sender, ws_receiver) = web_socket.split();

        Self {
            hb: Instant::now(),
            lapp_service_sender,
            ws_sender,
            ws_receiver,
        }
    }

    pub fn run(mut self, ctx: Context<Addr>, actor_id: Addr) {
        let mut messages_in = ctx.actor_receiver::<WsServiceMessage>(actor_id);
        let mut hb_interval = time::interval(Self::HEARTBEAT_INTERVAL);

        ctx.clone().spawn(async move {
            truba::event_loop!(ctx, {
                _ = hb_interval.tick() => {
                    if self.handle_heartbeat().await.is_break() {
                        break;
                    }
                }
                Some(msg) = self.ws_receiver.next() => {
                    if self.handle_ws_message(msg).is_break() {
                        break;
                    }
                },
                Some(WsServiceMessage(msg)) = messages_in.recv() => {
                    match msg {
                        WsMessage::Text(text) => if self.handle_text(text).await.is_break() {
                            break;
                        },
                    }
                }
            });
            self.close().await;
        });
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    async fn handle_heartbeat(&mut self) -> ControlFlow<(), ()> {
        // check client heartbeats
        if Instant::now().duration_since(self.hb) > Self::CLIENT_TIMEOUT {
            // heartbeat timed out
            log::debug!("Websocket Client heartbeat failed, disconnecting!");

            // don't try to send a ping
            return ControlFlow::Break(());
        }

        if let Err(err) = self.ws_sender.send(Message::Ping(Vec::new())).await {
            log::error!("WS error: {err:?}");
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn handle_ws_message(&mut self, msg: Result<Message, axum::Error>) -> ControlFlow<(), ()> {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                log::error!("WS error: {err:?}");
                return ControlFlow::Break(());
            },
        };

        match msg {
            Message::Text(text) => {
                log::info!("Receive WS message: {text}");
                if let Err(err) = self
                    .lapp_service_sender
                    .send(LappServiceMessage::WebSocket(WsMessage::Text(text)))
                {
                    log::error!("Error occurs when send to lapp service: {err:?}");
                }
            },
            Message::Binary(_bin) => {},
            Message::Close(_close_frame) => {
                return ControlFlow::Break(());
            },

            Message::Pong(_) => {
                self.hb = Instant::now();
            },
            // You should never need to manually handle Message::Ping, as axum's websocket library
            // will do so for you automagically by replying with Pong and copying the v according to
            // spec. But if you need the contents of the pings you can see them here.
            Message::Ping(_) => {
                self.hb = Instant::now();
            },
        }
        ControlFlow::Continue(())
    }

    async fn handle_text(&mut self, text: String) -> ControlFlow<(), ()> {
        if let Err(err) = self.ws_sender.send(Message::Text(text)).await {
            log::error!("WS send error: {err:?}");
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    async fn close(&mut self) {
        if let Err(err) = self.ws_sender.send(Message::Close(None)).await {
            log::error!("WS close send error: {err:?}");
        }
    }
}

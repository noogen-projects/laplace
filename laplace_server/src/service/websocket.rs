use std::io;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use axum::extract::ws;
use axum::extract::ws::WebSocket;
use derive_more::From;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt, TryStreamExt};
pub use laplace_wasm::route::websocket::{Message, MessageIn, MessageOut};
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
pub struct WsServiceMessage(pub MessageOut);

impl truba::Message for WsServiceMessage {
    type Channel = UnboundedMpscChannel<Self>;
}

#[derive(Debug)]
pub struct WebSocketService {
    /// Client must send ping at least once per SETTINGS.ws.client_timeout_sec seconds,
    /// otherwise we drop connection.
    hb: Instant,

    lapp_service_sender: Sender<LappServiceMessage>,
    ws_sender: SplitSink<WebSocket, ws::Message>,
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
                    if self.handle_service_message(msg).await.is_break() {
                        break;
                    }
                },
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
            self.send_to_lapp(MessageIn::Timeout);

            // don't try to send a ping
            ControlFlow::Break(())
        } else if !self.send_to_ws(None, ws::Message::Ping(Vec::new())).await {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn handle_ws_message(&mut self, msg: Result<ws::Message, axum::Error>) -> ControlFlow<(), ()> {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                log::error!("WS error: {err:?}");
                return ControlFlow::Break(());
            },
        };

        match msg {
            ws::Message::Text(text) => {
                log::debug!("Receive WS text: {text}");
                self.send_to_lapp(Message::Text(text).into());
            },
            ws::Message::Binary(bin) => {
                log::debug!("Receive WS binary: {bin:?}");
                self.send_to_lapp(Message::Binary(bin).into());
            },
            ws::Message::Close(close_frame) => {
                log::debug!("Receive WS close: {close_frame:?}");
                self.send_to_lapp(Message::Close.into());
                return ControlFlow::Break(());
            },

            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            },
            // You should never need to manually handle Message::Ping, as axum's websocket library
            // will do so for you automagically by replying with Pong and copying the v according to
            // spec. But if you need the contents of the pings you can see them here.
            ws::Message::Ping(_) => {
                self.hb = Instant::now();
            },
        }
        ControlFlow::Continue(())
    }

    async fn handle_service_message(&mut self, MessageOut { id, msg }: MessageOut) -> ControlFlow<(), ()> {
        let id = Some(id);
        let sent = match msg {
            Message::Text(text) => self.send_to_ws(id, ws::Message::Text(text)).await,
            Message::Binary(text) => self.send_to_ws(id, ws::Message::Binary(text)).await,
            Message::Close => self.send_to_ws(id, ws::Message::Close(None)).await,
        };
        if !sent {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    async fn close(&mut self) {
        self.ws_sender.send(ws::Message::Close(None)).await.ok();
    }

    async fn send_to_ws(&mut self, id: Option<String>, msg: ws::Message) -> bool {
        let sent;
        let result;

        if let Err(err) = self.ws_sender.send(msg).await {
            log::error!("WS send error: {err:?}");
            result = Err(err.to_string());
            sent = false;
        } else {
            result = Ok(());
            sent = true;
        }

        if let Some(id) = id {
            self.send_to_lapp(MessageIn::Response { id, result });
        } else if let Err(err) = result {
            self.send_to_lapp(MessageIn::Error(err.to_string()));
        }
        sent
    }

    fn send_to_lapp(&self, msg: MessageIn) {
        if let Err(err) = self.lapp_service_sender.send(LappServiceMessage::WebSocket(msg)) {
            log::error!("Error occurs when send to lapp service: {err:?}");
        }
    }
}

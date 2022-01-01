use std::{
    io,
    time::{Duration, Instant},
};

use actix::{Actor, ActorContext, AsyncContext, Handler, Running, StreamHandler, WrapFuture};
use actix_web_actors::ws;
use derive_more::From;
use log::{debug, error};
use wasmer::{ExportError, RuntimeError};

use crate::{lapps::LappInstanceError, service};

pub use laplace_wasm::route::websocket::Message;

#[derive(Debug, From)]
enum WsError {
    Export(ExportError),
    Runtime(RuntimeError),
    Instance(LappInstanceError),
    Io(io::Error),
}

pub struct ActixMessage(pub Message);

impl actix::Message for ActixMessage {
    type Result = ();
}

#[derive(Debug)]
pub struct WebSocketService {
    /// Client must send ping at least once per SETTINGS.ws.client_timeout_sec seconds,
    /// otherwise we drop connection.
    hb: Instant,

    lapp_service_sender: service::lapp::Sender,
}

impl WebSocketService {
    /// How often heartbeat pings are sent
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    /// How long before lack of client response causes a timeout
    const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn new(lapp_service_sender: service::lapp::Sender) -> Self {
        Self {
            hb: Instant::now(),
            lapp_service_sender,
        }
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn heartbeat(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(Self::HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > Self::CLIENT_TIMEOUT {
                // heartbeat timed out
                println!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for WebSocketService {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        self.heartbeat(ctx);
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        Running::Stop
    }
}

impl Handler<ActixMessage> for WebSocketService {
    type Result = ();

    fn handle(&mut self, msg: ActixMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg.0 {
            Message::Text(text) => ctx.text(text),
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketService {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                error!("WS error: {:?}", err);
                ctx.stop();
                return;
            },
        };

        // process websocket messages
        debug!("WS message: {:?}", msg);
        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            },
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            },
            ws::Message::Text(text) => {
                let lapp_service_sender = self.lapp_service_sender.clone();
                let fut = async move {
                    if let Err(err) = lapp_service_sender
                        .send(service::lapp::Message::WebSocket(Message::Text(text.to_string())))
                        .await
                    {
                        log::error!("Error occurs when send to lapp service: {:?}", err);
                    }
                };
                ctx.wait(fut.into_actor(self));
            },
            ws::Message::Binary(bin) => ctx.binary(bin),
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            },
            ws::Message::Continuation(_) => {
                ctx.stop();
            },
            ws::Message::Nop => (),
        }
    }
}

use std::time::{Duration, Instant};

use actix::{Actor, ActorContext, AsyncContext, Running, StreamHandler};
use actix_web_actors::ws;
use log::{debug, error};

pub struct WebSocketService {
    /// Client must send ping at least once per SETTINGS.ws.client_timeout_sec seconds,
    /// otherwise we drop connection.
    hb: Instant,
}

impl WebSocketService {
    /// How often heartbeat pings are sent
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    /// How long before lack of client response causes a timeout
    const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn new() -> Self {
        Self { hb: Instant::now() }
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

    fn handle_message(&self, msg: &str) -> String {
        msg.to_string()
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

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebSocketService {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                error!("WS error: {:?}", err);
                ctx.stop();
                return;
            }
        };

        // process websocket messages
        debug!("WS message: {:?}", msg);
        match msg {
            ws::Message::Ping(msg) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => ctx.text(
                serde_json::to_string(&self.handle_message(&text))
                    .as_deref()
                    .unwrap_or(r#""To json error""#),
            ),
            ws::Message::Binary(bin) => ctx.binary(bin),
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => {
                ctx.stop();
            }
            ws::Message::Nop => (),
        }
    }
}

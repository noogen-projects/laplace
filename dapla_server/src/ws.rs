use std::{
    io,
    time::{Duration, Instant},
};

use actix::{Actor, ActorContext, AsyncContext, Running, StreamHandler};
use actix_web_actors::ws;
use borsh::BorshDeserialize;
use dapla_wasm::{route, Route};
use derive_more::From;
use log::{debug, error};
use wasmer::{ExportError, RuntimeError};

use crate::daps::{DapInstanceError, ExpectedInstance};

#[derive(Debug, From)]
enum WsError {
    Export(ExportError),
    Runtime(RuntimeError),
    Instance(DapInstanceError),
    Io(io::Error),
}

impl WsError {
    fn to_json_string(&self) -> String {
        format!(r#"{{"Error":"{:?}"}}"#, self)
    }
}

pub struct WebSocketService {
    /// Client must send ping at least once per SETTINGS.ws.client_timeout_sec seconds,
    /// otherwise we drop connection.
    hb: Instant,

    dap_instance: ExpectedInstance,
}

impl WebSocketService {
    /// How often heartbeat pings are sent
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    /// How long before lack of client response causes a timeout
    const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

    pub fn new(dap_instance: ExpectedInstance) -> Self {
        Self {
            hb: Instant::now(),
            dap_instance,
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

    fn handle_message(&self, msg: &str) -> Result<Vec<Route>, WsError> {
        let route_ws_fn = self
            .dap_instance
            .exports
            .get_function("route_ws")?
            .native::<u64, u64>()?;
        let msg_arg = self.dap_instance.bytes_to_wasm_slice(msg)?;

        let response_slice = route_ws_fn.call(msg_arg.into())?;
        let bytes = unsafe { self.dap_instance.wasm_slice_to_vec(response_slice)? };
        let routes = BorshDeserialize::try_from_slice(&bytes)?;

        Ok(routes)
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
            ws::Message::Text(text) => match self.handle_message(&text) {
                Ok(routes) => {
                    for route in routes {
                        match route {
                            Route::Http(http) => {
                                error!("Http routing is not supported for WS: {:?}", http);
                            }
                            Route::Websocket(route::Websocket::Text(msg)) => ctx.text(msg),
                            Route::P2p(p2p) => {
                                todo!()
                            }
                        }
                    }
                }
                Err(err) => {
                    ctx.text(err.to_json_string());
                }
            },
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

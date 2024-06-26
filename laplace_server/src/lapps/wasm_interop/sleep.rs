use std::time::Duration;

use wasmtime::Caller;

use crate::lapps::wasm_interop::BoxedSendFuture;
use crate::lapps::Ctx;

pub fn invoke_sleep(_caller: Caller<Ctx>, (millis,): (u64,)) -> BoxedSendFuture<()> {
    Box::new(tokio::time::sleep(Duration::from_millis(millis)))
}

pub use dapla_wasm_macro::process_http as process;
pub use http::{self as types, HeaderMap, HeaderValue, Method, StatusCode, Uri, Version};

use std::io;

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::WasmSlice;

pub type Request = http::Request<Vec<u8>>;
pub type RequestBuilder = http::request::Builder;
pub type Response = http::Response<Vec<u8>>;
pub type ResponseBuilder = http::response::Builder;

pub type Result<T> = std::result::Result<T, Error>;
pub type InvokeResult<T> = std::result::Result<T, InvokeError>;

#[derive(Debug, Error, BorshDeserialize, BorshSerialize)]
pub enum InvokeError {
    #[error("Read from WASM error")]
    CanNotReadWasmData,

    #[error("HTTP request deserialization error")]
    FailDeserializeRequest,

    #[error("HTTP response building error: {0}")]
    FailBuildResponse(String),

    #[error("HTTP method \"{0}\" not allowed")]
    ForbiddenMethod(Method),

    #[error("HTTP host \"{0}\" not allowed")]
    ForbiddenHost(String),

    #[error("HTTP request error: {}, {1}", display_code(.0))]
    FailRequest(Option<StatusCode>, String),
}

fn display_code(code: &Option<StatusCode>) -> String {
    if let Some(code) = code {
        format!("{}", code)
    } else {
        format!("None")
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("HTTP request serialization error: {0:?}")]
    FailSerializeRequest(io::Error),

    #[error("HTTP request building error: {0}")]
    FailBuildRequest(String),

    #[error("HTTP response deserialization error: {0:?}")]
    FailDeserializeResponse(io::Error),

    #[error("HTTP response building error: {0}")]
    FailBuildResponse(String),

    #[error("HTTP invoke error: {0:?}")]
    FailInvoke(InvokeError),
}

extern "C" {
    fn invoke_http(sql_query: WasmSlice) -> WasmSlice;
}

pub fn invoke(request: Request) -> Result<Response> {
    let request_bytes = request.try_to_vec().map_err(Error::FailSerializeRequest)?;
    let response_bytes = unsafe { invoke_http(WasmSlice::from(request_bytes)).into_vec_in_wasm() };
    let response: InvokeResult<Response> =
        BorshDeserialize::try_from_slice(&response_bytes).map_err(Error::FailDeserializeResponse)?;
    response.map_err(Error::FailInvoke)
}

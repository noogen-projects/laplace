use std::io::{self, Read};
use std::iter::FromIterator;

use borsh::io::Write;
use borsh::{BorshDeserialize, BorshSerialize};
pub use http::header::{self, HeaderName};
pub use http::{self as types, HeaderMap, HeaderValue, Method, StatusCode, Uri, Version};
pub use laplace_wasm_macro::process_http as process;
use thiserror::Error;

pub use self::request::*;
pub use self::response::*;
use crate::WasmSlice;

pub mod request;
pub mod response;

pub type Result<T> = std::result::Result<T, Error>;
pub type InvokeResult<T> = std::result::Result<T, InvokeError>;

#[derive(Debug, Error, BorshDeserialize, BorshSerialize)]
pub enum InvokeError {
    #[error("HTTP context is empty")]
    EmptyContext,

    #[error("Read from WASM error")]
    CanNotReadWasmData,

    #[error("HTTP request deserialization error")]
    FailDeserializeRequest,

    #[error("HTTP response building error: {0}")]
    FailBuildResponse(String),

    #[error("HTTP method \"{0}\" not allowed")]
    ForbiddenMethod(String),

    #[error("HTTP host \"{0}\" not allowed")]
    ForbiddenHost(String),

    #[error("HTTP request error: {code}, {1}", code = display_code(.0))]
    FailRequest(Option<u16>, String),
}

fn display_code(code: &Option<u16>) -> String {
    if let Some(code) = code {
        format!("{code}")
    } else {
        "None".to_string()
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
    fn invoke_http(request: WasmSlice) -> WasmSlice;
}

pub fn invoke(request: Request) -> Result<Response> {
    let request_bytes = borsh::to_vec(&request).map_err(Error::FailSerializeRequest)?;
    let response_bytes = unsafe { invoke_http(WasmSlice::from(request_bytes)).into_vec_in_wasm() };
    let response: InvokeResult<Response> =
        BorshDeserialize::try_from_slice(&response_bytes).map_err(Error::FailDeserializeResponse)?;
    response.map_err(Error::FailInvoke)
}

fn serialize_version<W: Write>(version: Version, writer: &mut W) -> io::Result<()> {
    match version {
        Version::HTTP_09 => 9_u8,
        Version::HTTP_10 => 10,
        Version::HTTP_11 => 11,
        Version::HTTP_2 => 20,
        Version::HTTP_3 => 30,
        _ => return Err(io::Error::from(io::ErrorKind::Unsupported)),
    }
    .serialize(writer)
}

fn deserialize_version<R: Read>(reader: &mut R) -> io::Result<Version> {
    Ok(match u8::deserialize_reader(reader)? {
        9 => Version::HTTP_09,
        10 => Version::HTTP_10,
        11 => Version::HTTP_11,
        20 => Version::HTTP_2,
        30 => Version::HTTP_3,
        _ => return Err(io::Error::from(io::ErrorKind::Unsupported)),
    })
}

fn serialize_headers<W: Write>(headers: &HeaderMap, writer: &mut W) -> io::Result<()> {
    let headers: Vec<_> = headers
        .into_iter()
        .map(|(key, value)| (key.as_str().as_bytes(), value.as_bytes()))
        .collect();
    headers.serialize(writer)
}

fn deserialize_headers<R: Read>(reader: &mut R) -> io::Result<HeaderMap> {
    let mut headers = Vec::new();
    for (name, value) in Vec::<(Vec<u8>, Vec<u8>)>::deserialize_reader(reader)?.into_iter() {
        headers.push((
            HeaderName::from_bytes(&name).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?,
            HeaderValue::from_bytes(&value).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?,
        ));
    }
    Ok(HeaderMap::from_iter(headers))
}

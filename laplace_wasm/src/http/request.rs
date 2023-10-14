use std::fmt;
use std::io::{self, Read};
use std::str::FromStr;

use borsh::io::Write;
use borsh::{BorshDeserialize, BorshSerialize};
use http;

use super::{
    deserialize_headers, deserialize_version, serialize_headers, serialize_version, HeaderMap, HeaderValue, Method,
    Uri, Version,
};

pub type RequestBuilder = http::request::Builder;

#[derive(Default)]
pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub headers: HeaderMap<HeaderValue>,
    pub body: Vec<u8>,
}

impl Request {
    #[inline]
    pub fn new(body: impl Into<Vec<u8>>) -> Self {
        Self {
            body: body.into(),
            ..Default::default()
        }
    }
}

impl From<Request> for http::Request<Vec<u8>> {
    fn from(request: Request) -> Self {
        let Request {
            method,
            uri,
            version,
            headers,
            body,
        } = request;
        let (mut parts, body) = http::Request::new(body).into_parts();
        parts.method = method;
        parts.uri = uri;
        parts.version = version;
        parts.headers = headers;
        http::Request::from_parts(parts, body)
    }
}

impl From<http::Request<Vec<u8>>> for Request {
    fn from(request: http::Request<Vec<u8>>) -> Self {
        let (parts, body) = request.into_parts();
        Self {
            method: parts.method,
            uri: parts.uri,
            version: parts.version,
            headers: parts.headers,
            body,
        }
    }
}

impl BorshSerialize for Request {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let Self {
            method,
            uri,
            version,
            headers,
            body,
        } = self;
        method.as_str().serialize(writer)?;
        uri.to_string().serialize(writer)?;
        serialize_version(*version, writer)?;
        serialize_headers(headers, writer)?;
        body.serialize(writer)
    }
}

impl BorshDeserialize for Request {
    fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        let method = Method::from_str(&String::deserialize_reader(reader)?)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        let uri = Uri::from_str(&String::deserialize_reader(reader)?)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        let version = deserialize_version(reader)?;
        let headers = deserialize_headers(reader)?;
        let body = Vec::<u8>::deserialize_reader(reader)?;

        Ok(Self {
            method,
            uri,
            version,
            headers,
            body,
        })
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Request")
            .field("method", &self.method)
            .field("uri", &self.uri)
            .field("version", &self.version)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .finish()
    }
}

use std::io::Read;
use std::{fmt, io};

use borsh::io::Write;
use borsh::{BorshDeserialize, BorshSerialize};
use http;

use super::{
    deserialize_headers, deserialize_version, serialize_headers, serialize_version, HeaderMap, HeaderValue, StatusCode,
    Version,
};

pub type ResponseBuilder = http::response::Builder;

#[derive(Default)]
pub struct Response {
    pub status: StatusCode,
    pub version: Version,
    pub headers: HeaderMap<HeaderValue>,
    pub body: Vec<u8>,
}

impl Response {
    #[inline]
    pub fn new(body: impl Into<Vec<u8>>) -> Self {
        Self {
            body: body.into(),
            ..Default::default()
        }
    }
}

impl From<Response> for http::Response<Vec<u8>> {
    fn from(response: Response) -> Self {
        let Response {
            status,
            version,
            headers,
            body,
        } = response;
        let (mut parts, body) = http::Response::new(body).into_parts();
        parts.status = status;
        parts.version = version;
        parts.headers = headers;
        http::Response::from_parts(parts, body)
    }
}

impl From<http::Response<Vec<u8>>> for Response {
    fn from(response: http::Response<Vec<u8>>) -> Self {
        let (parts, body) = response.into_parts();
        Self {
            status: parts.status,
            version: parts.version,
            headers: parts.headers,
            body,
        }
    }
}

impl BorshSerialize for Response {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let Self {
            status,
            version,
            headers,
            body,
        } = self;
        status.as_u16().serialize(writer)?;
        serialize_version(*version, writer)?;
        serialize_headers(headers, writer)?;
        body.serialize(writer)
    }
}

impl BorshDeserialize for Response {
    fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        let status = StatusCode::from_u16(u16::deserialize_reader(reader)?)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
        let version = deserialize_version(reader)?;
        let headers = deserialize_headers(reader)?;
        let body = Vec::<u8>::deserialize_reader(reader)?;

        Ok(Self {
            status,
            version,
            headers,
            body,
        })
    }
}

impl fmt::Debug for Response {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Response")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .finish()
    }
}

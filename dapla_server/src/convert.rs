use std::iter::FromIterator;

use actix_web::HttpRequest;
use dapla_wasm::http;

pub fn to_wasm_http_request(request: &HttpRequest, body: Option<Vec<u8>>) -> http::Request {
    http::Request {
        method: request.method().clone(),
        uri: request.uri().clone(),
        version: request.version(),
        headers: http::HeaderMap::from_iter(request.headers().clone()),
        body: body.unwrap_or_default(),
    }
}

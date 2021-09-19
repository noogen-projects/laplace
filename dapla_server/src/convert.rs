use actix_web::HttpRequest;
use dapla_wasm::process::http::{self, types::request::Builder};

pub fn to_wasm_http_request(request: &HttpRequest, body: Option<Vec<u8>>) -> http::Request {
    let mut builder = Builder::new()
        .method(request.method())
        .uri(request.uri())
        .version(request.version());

    if let Some(headers) = builder.headers_mut() {
        headers.extend(request.headers().clone());
    }

    builder.body(body.unwrap_or_else(Vec::new)).unwrap()
}

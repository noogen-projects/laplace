use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use laplace_wasm::http;

use crate::error::ServerResult;

pub async fn to_wasm_http_request(request: Request<Body>) -> ServerResult<http::Request> {
    let (parts, body) = request.into_parts();
    let body = BodyExt::collect(body).await?.to_bytes();

    Ok(http::Request {
        method: parts.method,
        uri: parts.uri,
        version: parts.version,
        headers: parts.headers,
        body: body.into(),
    })
}

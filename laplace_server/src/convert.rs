use axum::body::Body;
use axum::http::Request;
use hyper::body;
use laplace_wasm::http;

use crate::error::ServerResult;

pub async fn to_wasm_http_request(request: Request<Body>) -> ServerResult<http::Request> {
    let (parts, body) = request.into_parts();
    let body = body::to_bytes(body).await?;

    Ok(http::Request {
        method: parts.method,
        uri: parts.uri,
        version: parts.version,
        headers: parts.headers,
        body: body.into(),
    })
}

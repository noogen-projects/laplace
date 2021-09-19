use dapla_wasm::process::{self, http};

#[process::http]
fn http(request: http::Request) -> http::Response {
    let mut body = String::from("Echo ");
    body.push_str(&request.uri().to_string());

    http::Response::new(body.into_bytes())
}

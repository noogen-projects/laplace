use laplace_wasm::http;

#[http::process]
fn http(request: http::Request) -> http::Response {
    let mut body = String::from("Echo ");
    body.push_str(&request.uri.to_string());

    http::Response::new(body.into_bytes())
}

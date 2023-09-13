use std::iter::FromIterator;
use std::time::Duration;

use borsh::{BorshDeserialize, BorshSerialize};
use laplace_common::lapp::{HttpHosts, HttpMethod, HttpMethods, HttpSettings};
use laplace_wasm::http;
use reqwest::Client;
use wasmtime::Caller;

use crate::lapps::wasm_interop::BoxedSendFuture;
use crate::lapps::Ctx;

#[derive(Clone)]
pub struct HttpCtx {
    pub client: Client,
    pub settings: HttpSettings,
}

impl HttpCtx {
    pub fn new(client: Client, settings: HttpSettings) -> Self {
        Self { client, settings }
    }
}

pub fn invoke_http(caller: Caller<Ctx>, request_slice: u64) -> BoxedSendFuture<u64> {
    Box::new(invoke_http_async(caller, request_slice))
}

pub async fn invoke_http_async(mut caller: Caller<'_, Ctx>, request_slice: u64) -> u64 {
    let memory_data = caller.data().memory_data().clone();

    let request_bytes = memory_data
        .to_manager(&mut caller)
        .wasm_slice_to_vec(request_slice)
        .await
        .map_err(|_| http::InvokeError::CanNotReadWasmData);

    let result = match caller.data().http.as_ref() {
        Some(http_ctx) => match request_bytes.and_then(|bytes| {
            BorshDeserialize::try_from_slice(&bytes).map_err(|_| http::InvokeError::FailDeserializeRequest)
        }) {
            Ok(request) => do_invoke_http(http_ctx, request).await,
            Err(err) => Err(err),
        },
        None => Err(http::InvokeError::EmptyContext),
    };

    let serialized = result.try_to_vec().expect("Result should be serializable");
    memory_data
        .to_manager(&mut caller)
        .bytes_to_wasm_slice(&serialized)
        .await
        .expect("Result should be to move to WASM")
        .into()
}

pub async fn do_invoke_http(ctx: &HttpCtx, request: http::Request) -> http::InvokeResult<http::Response> {
    log::debug!("Invoke HTTP: {request:#?},\n{:#?}", ctx.settings);
    let http::Request {
        method,
        uri,
        version,
        headers,
        body,
    } = request;

    log::debug!("Invoke HTTP body: {}", String::from_utf8_lossy(&body));

    if !is_method_allowed(&method, &ctx.settings.methods) {
        return Err(http::InvokeError::ForbiddenMethod(method.to_string()));
    }

    if !is_host_allowed(uri.host().unwrap_or(""), &ctx.settings.hosts) {
        return Err(http::InvokeError::ForbiddenHost(uri.host().unwrap_or("").into()));
    }

    match ctx
        .client
        .request(method, uri.to_string())
        .version(version)
        .body(body)
        .headers(headers)
        .timeout(Duration::from_millis(ctx.settings.timeout_ms))
        .send()
        .await
    {
        Ok(response) => {
            log::debug!("Invoke HTTP response: {response:#?}");

            Ok(http::Response {
                status: response.status(),
                version: response.version(),
                headers: http::HeaderMap::from_iter(
                    response
                        .headers()
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                ),
                body: {
                    let body = response.bytes().await.map(|bytes| bytes.to_vec()).unwrap_or_default();
                    log::debug!("Invoke HTTP response body: {}", String::from_utf8_lossy(&body));
                    body
                },
            })
        },
        Err(err) => Err(http::InvokeError::FailRequest(
            err.status().map(|status| status.as_u16()),
            format!("{}", err),
        )),
    }
}

fn is_method_allowed(method: &http::Method, methods: &HttpMethods) -> bool {
    match methods {
        HttpMethods::All => true,
        HttpMethods::List(list) => list.iter().any(|item| match item {
            HttpMethod::Get => method == http::Method::GET,
            HttpMethod::Post => method == http::Method::POST,
        }),
    }
}

fn is_host_allowed(host: &str, hosts: &HttpHosts) -> bool {
    match hosts {
        HttpHosts::All => true,
        HttpHosts::List(list) => list.iter().any(|item| item.as_str() == host),
    }
}

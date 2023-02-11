use std::iter::FromIterator;
use std::time::Duration;

use borsh::{BorshDeserialize, BorshSerialize};
use laplace_common::lapp::{HttpHosts, HttpMethod, HttpMethods, HttpSettings};
use laplace_wasm::http;
use reqwest::blocking::Client;
use wasmer::FunctionEnvMut;

use crate::lapps::wasm_interop::MemoryManagementHostData;

#[derive(Clone)]
pub struct HttpEnv {
    pub memory_data: Option<MemoryManagementHostData>,
    pub client: Client,
    pub settings: HttpSettings,
}

pub fn invoke_http(mut env: FunctionEnvMut<HttpEnv>, request_slice: u64) -> u64 {
    let memory_data = env.data().memory_data.clone().expect("Memory data must not be empty");

    let request_bytes = unsafe {
        memory_data
            .to_manager(&mut env)
            .wasm_slice_to_vec(request_slice)
            .map_err(|_| http::InvokeError::CanNotReadWasmData)
    };

    let result = request_bytes
        .and_then(|bytes| {
            BorshDeserialize::try_from_slice(&bytes).map_err(|_| http::InvokeError::FailDeserializeRequest)
        })
        .and_then(|request| do_invoke_http(&env.data().client, request, &env.data().settings));

    let serialized = result.try_to_vec().expect("Result should be serializable");
    memory_data
        .to_manager(&mut env)
        .bytes_to_wasm_slice(&serialized)
        .expect("Result should be to move to WASM")
        .into()
}

pub fn do_invoke_http(
    client: &Client,
    request: http::Request,
    settings: &HttpSettings,
) -> http::InvokeResult<http::Response> {
    log::debug!("Invoke HTTP: {request:#?},\n{settings:#?}");
    let http::Request {
        method,
        uri,
        version,
        headers,
        body,
    } = request;

    log::debug!("Invoke HTTP body: {}", String::from_utf8_lossy(&body));

    if !is_method_allowed(&method, &settings.methods) {
        return Err(http::InvokeError::ForbiddenMethod(method.to_string()));
    }

    if !is_host_allowed(uri.host().unwrap_or(""), &settings.hosts) {
        return Err(http::InvokeError::ForbiddenHost(uri.host().unwrap_or("").into()));
    }

    client
        .request(method, uri.to_string())
        .version(version)
        .body(body)
        .headers(headers)
        .timeout(Duration::from_millis(settings.timeout_ms))
        .send()
        .map_err(|err| http::InvokeError::FailRequest(err.status().map(|status| status.as_u16()), format!("{}", err)))
        .map(|response| {
            log::debug!("Invoke HTTP response: {response:#?}");

            http::Response {
                status: response.status(),
                version: response.version(),
                headers: http::HeaderMap::from_iter(
                    response
                        .headers()
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                ),
                body: {
                    let body = response.bytes().map(|bytes| bytes.to_vec()).unwrap_or_default();
                    log::debug!("Invoke HTTP response body: {}", String::from_utf8_lossy(&body));
                    body
                },
            }
        })
}

fn is_method_allowed(method: &http::Method, methods: &HttpMethods) -> bool {
    match methods {
        HttpMethods::All => true,
        HttpMethods::List(list) => list
            .iter()
            .find(|item| match item {
                HttpMethod::Get => method == http::Method::GET,
                HttpMethod::Post => method == http::Method::POST,
            })
            .is_some(),
    }
}

fn is_host_allowed(host: &str, hosts: &HttpHosts) -> bool {
    match hosts {
        HttpHosts::All => true,
        HttpHosts::List(list) => list.iter().find(|item| item.as_str() == host).is_some(),
    }
}

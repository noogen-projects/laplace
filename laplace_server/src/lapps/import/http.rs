use std::{borrow::Borrow, convert::TryFrom, iter::FromIterator, sync::Arc, time::Duration};

use arc_swap::ArcSwapOption;
use borsh::{BorshDeserialize, BorshSerialize};
use laplace_common::lapp::{HttpHosts, HttpMethod, HttpMethods, HttpSettings};
use laplace_wasm::http;
use reqwest::blocking::Client;
use wasmer::{Instance, WasmerEnv};

use crate::lapps::ExpectedInstance;

#[derive(WasmerEnv, Clone)]
pub struct HttpEnv {
    pub instance: Arc<ArcSwapOption<Instance>>,
    pub client: Client,
    pub settings: HttpSettings,
}

impl<T: Borrow<HttpEnv>> From<T> for ExpectedHttpEnv {
    fn from(env: T) -> Self {
        let env = env.borrow();
        let instance =
            ExpectedInstance::try_from(env.instance.load_full().expect("Lapp instance should be initialized"))
                .expect("Memory should be presented");

        Self {
            instance,
            client: env.client.clone(),
            settings: env.settings.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ExpectedHttpEnv {
    pub instance: ExpectedInstance,
    pub client: Client,
    pub settings: HttpSettings,
}

pub fn invoke_http(env: &HttpEnv, request_slice: u64) -> u64 {
    let env = ExpectedHttpEnv::from(env);
    let request_bytes = unsafe {
        env.instance
            .wasm_slice_to_vec(request_slice)
            .map_err(|_| http::InvokeError::CanNotReadWasmData)
    };

    let result = request_bytes
        .and_then(|bytes| {
            BorshDeserialize::try_from_slice(&bytes).map_err(|_| http::InvokeError::FailDeserializeRequest)
        })
        .and_then(|request| do_invoke_http(&env.client, request, &env.settings));

    let serialized = result.try_to_vec().expect("Result should be serializable");
    env.instance
        .bytes_to_wasm_slice(&serialized)
        .expect("Result should be to move to WASM")
        .into()
}

pub fn do_invoke_http(
    client: &Client,
    request: http::Request,
    settings: &HttpSettings,
) -> http::InvokeResult<http::Response> {
    log::debug!("Invoke HTTP: {:#?},\n{:#?}", request, settings);
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
            log::debug!("Invoke HTTP response: {:#?}", response);

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

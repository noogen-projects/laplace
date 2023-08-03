use std::fmt::Display;
use std::time::{Duration, Instant};

use reqwest::{Client, Response};
use strum::Display;
use tokio::time;

#[derive(Debug, Display, Clone, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum Scheme {
    Http,
    Https,
}

#[derive(Debug)]
pub struct LaplaceClientBuilder {
    request_timeout: Option<Duration>,
    scheme: Scheme,
    host: String,
    port: u16,
}

impl Default for LaplaceClientBuilder {
    fn default() -> Self {
        Self {
            request_timeout: None,
            scheme: Scheme::Http,
            host: "127.0.0.1".to_string(),
            port: 80,
        }
    }
}

impl LaplaceClientBuilder {
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    pub fn scheme(mut self, scheme: Scheme) -> Self {
        self.scheme = scheme;
        self
    }

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn build(self) -> reqwest::Result<LaplaceClient> {
        let mut builder = Client::builder().danger_accept_invalid_certs(true);
        if let Some(timeout) = self.request_timeout {
            builder = builder.timeout(timeout);
        }
        let client = builder.build()?;

        Ok(LaplaceClient { client, param: self })
    }
}

pub struct LaplaceClient {
    client: Client,
    param: LaplaceClientBuilder,
}

impl LaplaceClient {
    pub fn builder() -> LaplaceClientBuilder {
        LaplaceClientBuilder::default()
    }

    pub fn http(host: impl Into<String>, port: u16) -> LaplaceClientBuilder {
        Self::builder()
            .request_timeout(Duration::from_secs(10))
            .scheme(Scheme::Http)
            .host(host)
            .port(port)
    }

    pub fn https(host: impl Into<String>, port: u16) -> LaplaceClientBuilder {
        Self::builder()
            .request_timeout(Duration::from_secs(10))
            .scheme(Scheme::Https)
            .host(host)
            .port(port)
    }

    pub fn url(&self, path: impl Display) -> String {
        format!("{}://{}:{}/{path}", self.param.scheme, self.param.host, self.param.port)
    }

    pub async fn wait_to_ready(&self, timeout: Duration) -> reqwest::Result<()> {
        let instant = Instant::now();
        while let Err(err) = self.get_index().await {
            if !err.is_connect() || instant.elapsed() >= timeout {
                return Err(err);
            }
            time::sleep(timeout / 1000).await;
        }
        Ok(())
    }

    pub async fn get_index(&self) -> reqwest::Result<Response> {
        self.client.get(self.url("")).send().await
    }

    pub async fn get_laplace(&self) -> reqwest::Result<Response> {
        self.client.get(self.url("laplace")).send().await
    }
}

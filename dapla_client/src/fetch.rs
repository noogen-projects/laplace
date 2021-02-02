use anyhow::{Context, Result};
use yew::{
    format::{Nothing, Text},
    services::{
        fetch::{FetchService, FetchTask, Request},
        Task,
    },
    Callback,
};

use crate::{DapResponse, StringResponse};

pub struct Fetcher {
    tasks: Vec<FetchTask>,
}

impl Fetcher {
    pub fn new() -> Self {
        Self { tasks: vec![] }
    }

    pub fn parse(response: StringResponse) -> Result<DapResponse> {
        let body = response.into_body()?;
        serde_json::from_str(body.as_str()).map_err(Into::into)
    }

    pub fn fetch(&mut self, request: Request<impl Into<Text>>, callback: Callback<StringResponse>) -> Result<()> {
        let task = FetchService::fetch(request, callback)?;
        self.tasks.retain(|task| task.is_active());
        self.tasks.push(task);
        Ok(())
    }

    pub fn send_get(&mut self, uri: impl AsRef<str>, callback: Callback<StringResponse>) -> Result<()> {
        let request = Request::get(uri.as_ref())
            .body(Nothing)
            .context("Create get request error")?;
        self.fetch(request, callback).context("Fetch get response error")
    }

    pub fn send_post(
        &mut self,
        uri: impl AsRef<str>,
        body: impl Into<String>,
        callback: Callback<StringResponse>,
    ) -> Result<()> {
        let request = Request::post(uri.as_ref())
            .body(Ok(body.into()))
            .context("Create post request error")?;
        self.fetch(request, callback).context("Fetch post response error")
    }
}

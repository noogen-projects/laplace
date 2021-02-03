use anyhow::{anyhow, Context, Error, Result};
use serde::Deserialize;
use yew::{
    format::{Nothing, Text},
    services::{
        fetch::{FetchService, FetchTask, Request, Response},
        Task,
    },
    Callback, Component, ComponentLink,
};

pub type StringResponse = Response<Result<String>>;

pub struct JsonFetcher {
    tasks: Vec<FetchTask>,
}

impl JsonFetcher {
    pub fn new() -> Self {
        Self { tasks: vec![] }
    }

    pub fn parse<R>(response: StringResponse) -> Result<R>
    where
        R: for<'a> Deserialize<'a>,
    {
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

    pub fn callback<Comp, Resp, Msg, FnMap, FnMapErr>(
        link: &ComponentLink<Comp>,
        map: FnMap,
        map_err: FnMapErr,
    ) -> Callback<StringResponse>
    where
        Comp: Component<Message = Msg>,
        Resp: for<'de> Deserialize<'de>,
        FnMap: Fn(Resp) -> Msg + 'static,
        FnMapErr: Fn(Error) -> Msg + 'static,
    {
        link.callback(move |response: StringResponse| {
            if response.status().is_success() {
                match JsonFetcher::parse(response).context("Can't parse response") {
                    Ok(response) => map(response),
                    Err(err) => map_err(err),
                }
            } else {
                map_err(anyhow!(
                    "Fetch status: {:?}, body: {:?}",
                    response.status(),
                    response.into_body()
                ))
            }
        })
    }
}

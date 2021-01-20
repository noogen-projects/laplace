use anyhow::Error;
use dapla_common::dap::Dap as CommonDap;
use yew::{
    format::{Nothing, Text},
    html, initialize, run_loop,
    services::{
        fetch::{FetchService, FetchTask, Request, Response},
        ConsoleService, Task,
    },
    utils, App, Callback, Component, ComponentLink, Html,
};
use yew_mdc_widgets::{auto_init, Drawer, IconButton, MdcWidget, Switch, TopAppBar};

type Dap = CommonDap<String>;

type StringResponse = Response<Result<String, Error>>;

struct Fetcher {
    tasks: Vec<FetchTask>,
}

impl Fetcher {
    fn new() -> Self {
        Self { tasks: vec![] }
    }

    fn fetch(&mut self, request: Request<impl Into<Text>>, callback: Callback<StringResponse>) -> Result<(), Error> {
        let task = FetchService::fetch(request, callback)?;
        self.tasks.retain(|task| task.is_active());
        self.tasks.push(task);
        Ok(())
    }
}

struct Root {
    daps: Vec<Dap>,
    link: ComponentLink<Self>,
    fetcher: Fetcher,
}

enum Msg {
    Fetch(StringResponse),
    SwitchDap(String),
    Sent,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let daps_list_request = Request::get("/daps").body(Nothing).expect("Request should be built");
        let mut fetcher = Fetcher::new();
        fetcher
            .fetch(daps_list_request, link.callback(Msg::Fetch))
            .expect("Fetch daps list error");

        Self {
            daps: vec![],
            link,
            fetcher,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Fetch(response) => {
                if response.status().is_success() {
                    let body = response.into_body();
                    let json = body.as_ref().map(|text| text.as_str()).unwrap_or_else(|_| "[]");
                    serde_json::from_str(json)
                        .map(|daps| {
                            self.daps = daps;
                            true
                        })
                        .unwrap_or_else(|err| {
                            ConsoleService::error(&format!("Response parsing error: {:?}, body: {:?}", err, json));
                            false
                        })
                } else {
                    ConsoleService::error(&format!(
                        "Response status: {:?}, body: {:?}",
                        response.status(),
                        response.into_body()
                    ));
                    false
                }
            }
            Msg::SwitchDap(name) => {
                if let Some(dap) = self.daps.iter_mut().find(|dap| dap.name() == name) {
                    dap.switch_enabled();

                    let uri = format!("/dap/{}", dap.name());
                    let body = if dap.enabled() {
                        "{\"enabled\":true}"
                    } else {
                        "{\"enabled\":false}"
                    };
                    self.send_post(uri, body);

                    true
                } else {
                    ConsoleService::error(&format!("Unknown dap name: {}", name));
                    false
                }
            }
            Msg::Sent => false,
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let drawer_id = "app-drawer";
        let drawer = Drawer::new()
            .id(drawer_id)
            .title(html! { <h3 tabindex = 0>{ "Settings" }</h3> })
            .modal();

        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("dapla")
            .navigation_item(IconButton::new().icon("menu"))
            .enable_shadow_when_scroll_window()
            .add_navigation_event(format!(
                r"{{
                    const drawer = document.getElementById('{}').MDCDrawer;
                    drawer.open = !drawer.open;
                }}",
                drawer_id
            ));

        html! {
            <>
                { drawer }
                <div class="mdc-drawer-scrim"></div>

                <div class = vec!["app-content", Drawer::APP_CONTENT_CLASS]>
                    { top_app_bar }

                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Applications" }</h1>
                            <div class = "daps-table">
                                { self.daps.iter().map(|dap| self.view_dap(dap)).collect::<Html>() }
                            </div>
                        </div>
                    </div>
                </div>
            </>
        }
    }

    fn rendered(&mut self, _first_render: bool) {
        auto_init();
    }
}

impl Root {
    fn send_post(&mut self, uri: impl AsRef<str>, body: impl Into<String>) {
        match Request::post(uri.as_ref()).body(Ok(body.into())) {
            Ok(request) => {
                self.fetcher
                    .fetch(
                        request,
                        self.link.callback(|response: StringResponse| {
                            if !response.status().is_success() {
                                ConsoleService::error(&format!(
                                    "Response status: {:?}, body: {:?}",
                                    response.status(),
                                    response.into_body()
                                ));
                            }
                            Msg::Sent
                        }),
                    )
                    .map_err(|err| ConsoleService::error(&format!("Fetch error: {:?}", err)))
                    .ok();
            }
            Err(err) => ConsoleService::error(&format!("Create post request error: {:?}", err)),
        }
    }

    fn view_dap(&self, dap: &Dap) -> Html {
        let dap_name = dap.name().to_string();
        let mut switch = Switch::new().on_click(self.link.callback(move |_| Msg::SwitchDap(dap_name.clone())));
        if dap.enabled() {
            switch = switch.on();
        }
        html! {
            <div class = "daps-table-row">
                <div class = "daps-table-col">
                    <big><a href = dap.name()>{ dap.title() }</a></big>
                </div>
                <div class = "daps-table-col">
                    { switch }
                </div>
            </div>
        }
    }
}

fn main() {
    initialize();
    if let Ok(Some(root)) = utils::document().query_selector("#root") {
        App::<Root>::new().mount_with_props(root, ());
        run_loop();
    } else {
        ConsoleService::error("Can't get root node for rendering");
    }
}

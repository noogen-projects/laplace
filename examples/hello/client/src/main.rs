use anyhow::Error;
use yew::{
    format::Nothing,
    html, initialize, run_loop,
    services::{
        console::ConsoleService,
        fetch::{FetchService, FetchTask, Request, Response},
    },
    utils, App, Component, ComponentLink, Html,
};
use yew_mdc_widgets::{auto_init, MdcWidget, TopAppBar};

struct Root {
    header: String,
    link: ComponentLink<Self>,
    task: Option<FetchTask>,
}

enum Msg {
    Fetch(String),
    Error,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            header: "Hello".to_string(),
            link,
            task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Fetch(data) => {
                self.header = data;
                true
            }
            Msg::Error => false,
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("Hello")
            .enable_shadow_when_scroll_window();

        html! {
            <>
                <div class = "app-content">
                    { top_app_bar }
                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ &self.header }</h1>
                        </div>
                    </div>
                </div>
            </>
        }
    }

    fn rendered(&mut self, _first_render: bool) {
        auto_init();

        if self.task.is_some() {
            return;
        }

        if let Ok(request) = Request::get("/hello/fetch/test/").body(Nothing) {
            if let Ok(task) = FetchService::fetch(
                request,
                self.link.callback(|response: Response<Result<String, Error>>| {
                    if response.status().is_success() {
                        Msg::Fetch(response.into_body().unwrap_or_else(|_| String::new()))
                    } else {
                        ConsoleService::error(&format!(
                            "Fetch status: {:?}, body: {:?}",
                            response.status(),
                            response.into_body()
                        ));
                        Msg::Error
                    }
                }),
            )
            .map_err(|err| ConsoleService::error(&format!("Fetch error: {:?}", err)))
            {
                self.task.replace(task);
            }
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

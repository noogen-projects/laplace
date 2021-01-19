use anyhow::Error;
use dapla_common::dap::{Dap as CommonDap, DapSettings};
use yew::{
    format::Nothing,
    html, initialize, run_loop,
    services::{
        fetch::{FetchService, FetchTask, Request, Response},
        ConsoleService,
    },
    utils, App, Component, ComponentLink, Html,
};
use yew_mdc_widgets::{auto_init, Drawer, IconButton, MdcWidget, TopAppBar};

type Dap = CommonDap<String>;

struct Root {
    daps: Vec<Dap>,
    link: ComponentLink<Self>,
    _fetch_task: FetchTask,
}

enum Msg {
    Fetch(Response<Result<String, Error>>),
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let daps_list_request = Request::get("/daps").body(Nothing).expect("Request should be built");
        let _fetch_task =
            FetchService::fetch(daps_list_request, link.callback(Msg::Fetch)).expect("Fetch daps list error");

        Self {
            daps: vec![],
            link,
            _fetch_task,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Fetch(response) => {
                if response.status().is_success() {
                    let json = response.into_body().unwrap_or_else(|_| "[]".to_string());
                    serde_json::from_str(&json)
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
                    <script>{ format!(r"
                        const listEl = document.querySelector('.mdc-drawer .mdc-list');
                        listEl.addEventListener('click', (event) => {{
                            document.getElementById('{}').MDCDrawer.open = false;
                        }});
                    ", drawer_id) }</script>

                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Applications" }</h1>
                            { self.daps.iter().map(|dap| self.view_dap(dap)).collect::<Html>() }
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
    fn view_dap(&self, dap: &Dap) -> Html {
        html! {
            <h2 class = "mdc-typography--headline6"><a href = dap.name()>{ dap.title() }</a></h2>
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

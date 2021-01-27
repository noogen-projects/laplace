#![recursion_limit = "256"]

use anyhow::Error;
use web_sys::HtmlInputElement;
use yew::{
    format::Nothing,
    html, initialize, run_loop,
    services::{
        console::ConsoleService,
        fetch::{FetchService, FetchTask, Request, Response},
    },
    App, Component, ComponentLink, Html,
};
use yew_mdc_widgets::{auto_init, utils::dom, Button, List, ListItem, MdcWidget, TextField, TopAppBar};

struct Root {
    link: ComponentLink<Self>,
    fetch_task: Option<FetchTask>,
    responses: Vec<String>,
}

enum Msg {
    Submit,
    Fetch(String),
    Error,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            link,
            fetch_task: None,
            responses: Vec::new(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Submit => {
                if !self.fetch_task.is_some() {
                    let uri = dom::select_exist_element::<HtmlInputElement>("#uri > input").value();
                    if let Ok(request) = Request::get(format!("/echo/{}", uri)).body(Nothing) {
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
                            self.fetch_task.replace(task);
                        }
                    }
                }
                false
            }
            Msg::Fetch(data) => {
                self.fetch_task = None;
                self.responses.push(data);
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
            .title("Echo dap")
            .enable_shadow_when_scroll_window();

        let mut list = List::ul().divider();
        for uri in self.responses.iter().rev() {
            list = list.item(ListItem::new().text(uri)).divider();
        }

        html! {
            <>
                <div class = "app-content">
                    { top_app_bar }
                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Echo" }</h1>
                            <div class = "mdc-layout-grid">
                                <div class = "mdc-layout-grid__inner">
                                    <div class = "mdc-layout-grid__cell mdc-layout-grid__cell--span-4 mdc-layout-grid__cell--align-bottom">
                                        { TextField::filled().id("uri").class("expand").label("URI") }
                                    </div>
                                    <div class = "mdc-layout-grid__cell mdc-layout-grid__cell--span-1 mdc-layout-grid__cell--align-bottom">
                                        { Button::raised().label("submit").on_click(self.link.callback(|_| Msg::Submit)) }
                                    </div>
                                </div>
                            </div>
                            { list }
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

fn main() {
    initialize();
    if let Ok(Some(root)) = yew::utils::document().query_selector("#root") {
        App::<Root>::new().mount_with_props(root, ());
        run_loop();
    } else {
        ConsoleService::error("Can't get root node for rendering");
    }
}

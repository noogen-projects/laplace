#![recursion_limit = "256"]

use std::iter::FromIterator;

use anyhow::{anyhow, Context, Error};
use dapla_yew::{JsonFetcher, MsgError, RawHtml};
use notes_common::{Note, NoteContent, Response};
use pulldown_cmark::{html as cmark_html, Options, Parser};
use yew::{html, initialize, run_loop, services::console::ConsoleService, App, Component, ComponentLink, Html};
use yew_mdc_widgets::{auto_init, Card, CardContent, IconButton, MdcWidget, TopAppBar};

struct Root {
    link: ComponentLink<Self>,
    fetcher: JsonFetcher,
    notes: Vec<Note>,
}

enum Msg {
    Fetch(Response),
    Error(Error),
}

impl From<Error> for Msg {
    fn from(err: Error) -> Self {
        Self::Error(err)
    }
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut fetcher = JsonFetcher::new();
        fetcher
            .send_get("/notes/list", JsonFetcher::callback(&link, Msg::Fetch, Msg::Error))
            .context("Get notes list error")
            .msg_error(&link);

        Self {
            link,
            fetcher,
            notes: Vec::new(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Fetch(Response::Notes(notes)) => {
                self.notes = notes;
                true
            }
            Msg::Fetch(Response::Note(_)) => false,
            Msg::Fetch(Response::Error(err)) => {
                self.link.send_message(Msg::Error(anyhow!("{}", err)));
                false
            }
            Msg::Error(err) => {
                ConsoleService::error(&format!("{}", err));
                true
            }
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("Notes dap example")
            .enable_shadow_when_scroll_window();

        let note_cards: Vec<_> = self
            .notes
            .iter()
            .map(|note| {
                Card::new(&note.name)
                    .content(CardContent::primary_action(html! {
                        <div class = "note-card__content">
                            { to_html(&note.content) }
                        </div>
                    }))
                    .content(CardContent::actions().action_icons(Html::from_iter(vec![
                        IconButton::new().class(CardContent::ACTION_ICON_CLASSES).icon("edit"),
                        IconButton::new().class(CardContent::ACTION_ICON_CLASSES).toggle("star", "star_border"),
                    ])))
            })
            .collect();

        html! {
            <>
                <div class = "app-content">
                    { top_app_bar }
                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Notes" }</h1>

                            <div class = "notes mdc-layout-grid">
                                <div class = "mdc-layout-grid__inner">
                                    { for note_cards.into_iter().map(|card| html! { <div class = "mdc-layout-grid__cell">{ card }</div> }) }
                                </div>
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

fn to_html(content: &NoteContent) -> Html {
    let parser = new_cmark_parser(content.content());

    let mut html_output = String::new();
    cmark_html::push_html(&mut html_output, parser);

    html! { <RawHtml inner_html = html_output /> }
}

fn new_cmark_parser(source: &str) -> Parser {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    Parser::new_ext(source, options)
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

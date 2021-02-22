#![recursion_limit = "256"]

use std::{
    iter::FromIterator,
    ops::{Deref, DerefMut},
};

use anyhow::{anyhow, Context, Error};
use dapla_yew::{JsonFetcher, MsgError, RawHtml};
use lew::SimpleEditor;
use notes_common::{Note, NoteContent, Response};
use pulldown_cmark::{html as cmark_html, Options, Parser};
use web_sys::HtmlElement;
use yew::{html, initialize, run_loop, services::console::ConsoleService, App, Component, ComponentLink, Html};
use yew_mdc_widgets::{
    auto_init,
    utils::dom::{self, JsObjectAccess},
    Button, Card, CardContent, CustomEvent, Dialog, IconButton, MdcWidget, TopAppBar,
};

struct ModifiableNote(Note);

impl From<Note> for ModifiableNote {
    fn from(note: Note) -> Self {
        Self(note)
    }
}

impl Deref for ModifiableNote {
    type Target = Note;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ModifiableNote {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

enum FullNote {
    Initial(Note),
    Modifiable(ModifiableNote),
}

impl Deref for FullNote {
    type Target = Note;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Initial(note) => note,
            Self::Modifiable(note) => &*note,
        }
    }
}

struct Root {
    link: ComponentLink<Self>,
    fetcher: JsonFetcher,
    notes: Vec<FullNote>,
    current_note_index: Option<usize>,
    current_mode: Mode,
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    View,
    Edit,
}

enum Msg {
    OpenViewNote(String),
    OpenEditNote(String),
    ViewCurrentNote,
    EditCurrentNote,
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
            current_note_index: None,
            current_mode: Mode::View,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::OpenViewNote(name) => {
                self.current_mode = Mode::View;
                self.fetcher
                    .send_get(
                        format!("/notes/note/{}", name),
                        JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                    )
                    .context("Get note error")
                    .msg_error(&self.link);
                false
            }
            Msg::OpenEditNote(name) => {
                self.current_mode = Mode::Edit;
                self.fetcher
                    .send_get(
                        format!("/notes/note/{}", name),
                        JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                    )
                    .context("Get note error")
                    .msg_error(&self.link);
                false
            }
            Msg::ViewCurrentNote => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    let view_element = dom::get_exist_element_by_id::<HtmlElement>("note-dialog__view");
                    view_element.set_inner_html(&to_view_inner_html(&note.content));
                    view_element.class_list().remove_1("hidden").ok();

                    let edit_element = dom::get_exist_element_by_id::<HtmlElement>("note-dialog__edit");
                    if !edit_element.class_list().contains("hidden") {
                        edit_element.class_list().add_1("hidden").ok();
                    }

                    IconButton::set_on_by_id("switch_mode", false);
                    Dialog::open_existing("note-dialog");
                }
                false
            }
            Msg::EditCurrentNote => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    let textarea_element = dom::select_exist_element::<HtmlElement>("#note-dialog__edit > textarea");
                    textarea_element.set_inner_html(note.content.content());

                    let edit_element = dom::get_exist_element_by_id::<HtmlElement>("note-dialog__edit");
                    edit_element.class_list().remove_1("hidden").ok();

                    let view_element = dom::get_exist_element_by_id::<HtmlElement>("note-dialog__view");
                    if !view_element.class_list().contains("hidden") {
                        view_element.class_list().add_1("hidden").ok();
                    }

                    IconButton::set_on_by_id("switch_mode", true);
                    Dialog::open_existing("note-dialog");
                }
                false
            }
            Msg::Fetch(Response::Notes(notes)) => {
                self.notes = notes.into_iter().map(FullNote::Initial).collect();
                true
            }
            Msg::Fetch(Response::Note(note)) => {
                for (i, full_note) in self.notes.iter_mut().enumerate() {
                    if full_note.name == note.name {
                        *full_note = FullNote::Initial(note);
                        self.current_note_index = Some(i);
                        break;
                    }
                }
                match self.current_mode {
                    Mode::View => self.link.send_message(Msg::ViewCurrentNote),
                    Mode::Edit => self.link.send_message(Msg::EditCurrentNote),
                }
                false
            }
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

        let note_dialog = self.view_note();

        let note_cards: Vec<_> = self
            .notes
            .iter()
            .map(|note| {
                Card::new(&note.name)
                    .content(CardContent::primary_action(html! {
                        <div class = "note-card__content" onclick = self.link.callback({
                            let name = note.name.clone();
                            move |_| Msg::OpenViewNote(name.clone())
                        })>
                            { to_html(&note.content) }
                        </div>
                    }))
                    .content(CardContent::actions().action_icons(Html::from_iter(vec![
                            IconButton::new()
                                .class(CardContent::ACTION_ICON_CLASSES)
                                .icon("edit")
                                .on_click(self.link.callback({
                                    let name = note.name.clone();
                                    move |_| Msg::OpenEditNote(name.clone())
                                })),
                            IconButton::new()
                                .class(CardContent::ACTION_ICON_CLASSES)
                                .toggle("star", "star_border"),
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

                            { note_dialog }

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

impl Root {
    fn view_note(&self) -> Html {
        let switch_mode_button = IconButton::new()
            .id("switch_mode")
            .class(CardContent::ACTION_ICON_CLASSES)
            .toggle("visibility", "edit")
            .on_change(self.link.callback(|event: CustomEvent| {
                if event.detail().get("isOn").as_bool().unwrap_or(false) {
                    Msg::EditCurrentNote
                } else {
                    Msg::ViewCurrentNote
                }
            }));

        Dialog::new()
            .id("note-dialog")
            .content_item(html! {
                <>
                    { switch_mode_button }
                    <div id = "note-dialog__view" class = "hidden"></div>
                    <SimpleEditor id = "note-dialog__edit" class = "lew-simple hidden" placeholder = "Leave a content" cols = 40 />
                </>
            })
            .action(
                Button::new()
                    .label("Cancel")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("note-dialog")),
            )
            .into()
    }
}

fn to_view_inner_html(content: &NoteContent) -> String {
    let parser = new_cmark_parser(content.content());

    let mut html = String::new();
    cmark_html::push_html(&mut html, parser);

    html
}

fn to_html(content: &NoteContent) -> Html {
    html! { <RawHtml inner_html = to_view_inner_html(content) /> }
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

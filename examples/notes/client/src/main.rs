#![recursion_limit = "256"]

use std::{iter::FromIterator, ops::Deref};

use anyhow::{anyhow, Context, Error};
use dapla_yew::{JsonFetcher, MsgError, RawHtml};
use lew::SimpleEditor;
use notes_common::{Note, NoteContent, Response};
use pulldown_cmark::{html as cmark_html, Options, Parser};
use web_sys::{Element, HtmlElement, HtmlTextAreaElement};
use yew::{
    html, initialize, run_loop, services::console::ConsoleService, App, Component, ComponentLink, Html, InputData,
};
use yew_mdc_widgets::{
    auto_init,
    utils::dom::{self, JsObjectAccess},
    Button, Card, CardContent, CustomEvent, Dialog, IconButton, MdcWidget, TopAppBar,
};

struct FullNote {
    note: Note,
    is_modified: bool,
}

impl FullNote {
    fn initial(note: Note) -> Self {
        Self {
            note,
            is_modified: false,
        }
    }

    fn note_mut(&mut self) -> &mut Note {
        self.is_modified = true;
        &mut self.note
    }

    fn is_modified(&self) -> bool {
        self.is_modified
    }
}

impl Deref for FullNote {
    type Target = Note;

    fn deref(&self) -> &Self::Target {
        match self {
            Self { note, .. } => note,
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
    Observe,
}

enum Msg {
    GetInitialNote(String),
    OpenViewNote(String),
    OpenEditNote(String),
    ViewCurrentNote,
    EditCurrentNote,
    EditContent(String),
    Updated,
    DiscardChanges,
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
            current_mode: Mode::Observe,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::GetInitialNote(name) => {
                self.fetcher
                    .send_get(
                        format!("/notes/note/{}", name),
                        JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                    )
                    .context("Get note error")
                    .msg_error(&self.link);
                false
            }
            Msg::OpenViewNote(name) => {
                if let Some(index) = self.notes.iter().position(|note| note.name == name) {
                    if self.notes[index].is_modified() {
                        self.current_note_index = Some(index);
                        self.link.send_message(Msg::ViewCurrentNote);
                        return false;
                    }
                }

                self.current_mode = Mode::View;
                self.link.send_message(Msg::GetInitialNote(name));
                false
            }
            Msg::OpenEditNote(name) => {
                if let Some(index) = self.notes.iter().position(|note| note.name == name) {
                    if self.notes[index].is_modified() {
                        self.current_note_index = Some(index);
                        self.link.send_message(Msg::EditCurrentNote);
                        return false;
                    }
                }

                self.current_mode = Mode::Edit;
                self.link.send_message(Msg::GetInitialNote(name));
                false
            }
            Msg::ViewCurrentNote => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    dom::get_exist_element_by_id::<HtmlElement>("note-dialog__view").set_inner_html(
                        &to_view_inner_html(&note.content.content().expect("Content should be present")),
                    );

                    show_element("note-dialog__view");
                    hide_element("note-dialog__edit");

                    if note.is_modified() {
                        show_element("save-note-button");
                        show_element("discard-note-button");
                    } else {
                        hide_element("save-note-button");
                        hide_element("discard-note-button");
                    }

                    IconButton::set_on_by_id("edit_mode", false);
                    Dialog::open_existing("note-dialog");
                }
                false
            }
            Msg::EditCurrentNote => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    dom::select_exist_element::<HtmlTextAreaElement>("#note-dialog__edit > textarea")
                        .set_value(note.content.content().expect("Content should be present"));

                    show_element("note-dialog__edit");
                    hide_element("note-dialog__view");

                    if note.is_modified() {
                        show_element("save-note-button");
                        show_element("discard-note-button");
                    } else {
                        hide_element("save-note-button");
                        hide_element("discard-note-button");
                    }

                    IconButton::set_on_by_id("edit_mode", true);
                    Dialog::open_existing("note-dialog");
                }
                false
            }
            Msg::EditContent(content) => {
                let index = self.current_note_index.expect("Index should be presented");
                if !self.notes[index].is_modified() {
                    show_element("save-note-button");
                    show_element("discard-note-button");
                }
                self.notes[index].note_mut().content = NoteContent::FullBody(content);
                false
            }
            Msg::Updated => true,
            Msg::DiscardChanges => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    self.link.send_message(Msg::GetInitialNote(note.name.clone()));
                }
                false
            }
            Msg::Fetch(Response::Notes(notes)) => {
                self.notes = notes.into_iter().map(FullNote::initial).collect();
                true
            }
            Msg::Fetch(Response::Note(note)) => {
                for (i, full_note) in self.notes.iter_mut().enumerate() {
                    if full_note.name == note.name {
                        *full_note = FullNote::initial(note);
                        self.current_note_index = Some(i);
                        break;
                    }
                }
                match self.current_mode {
                    Mode::View => {
                        self.link.send_message(Msg::ViewCurrentNote);
                        false
                    }
                    Mode::Edit => {
                        self.link.send_message(Msg::EditCurrentNote);
                        false
                    }
                    Mode::Observe => true,
                }
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
                            { to_preview_html(&note.content) }
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
            .id("edit_mode")
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
                    <SimpleEditor id = "note-dialog__edit" class = "lew-simple hidden" placeholder = "Leave a content"
                            cols = 40 oninput = self.link.callback(|data: InputData| Msg::EditContent(data.value)) />
                </>
            })
            .action(
                Button::new()
                    .id("save-note-button")
                    .class("hidden")
                    .label("Save")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("note-dialog")),
            )
            .action(
                Button::new()
                    .id("discard-note-button")
                    .class("hidden")
                    .label("Discard")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(self.link.callback(|_| Msg::DiscardChanges)),
            )
            .on_closed(self.link.callback(|_| Msg::Updated))
            .into()
    }
}

fn to_view_inner_html(content: &str) -> String {
    let parser = new_cmark_parser(content);

    let mut html = String::new();
    cmark_html::push_html(&mut html, parser);

    html
}

fn to_preview_html(content: &NoteContent) -> Html {
    let preview = content.make_preview();
    html! { <RawHtml inner_html = to_view_inner_html(&preview) /> }
}

fn new_cmark_parser(source: &str) -> Parser {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    Parser::new_ext(source, options)
}

fn show_element(id: impl AsRef<str>) {
    dom::get_exist_element_by_id::<Element>(id.as_ref())
        .class_list()
        .remove_1("hidden")
        .ok();
}

fn hide_element(id: impl AsRef<str>) {
    let element = dom::get_exist_element_by_id::<Element>(id.as_ref());
    if !element.class_list().contains("hidden") {
        element.class_list().add_1("hidden").ok();
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

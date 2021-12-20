#![recursion_limit = "256"]

use std::ops::Deref;

use anyhow::{anyhow, Context, Error};
use laplace_yew::{JsonFetcher, MsgError, RawHtml};
use lew::SimpleEditor;
use notes_common::{Note, NoteContent, Response};
use pulldown_cmark::{html as cmark_html, Options, Parser};
use web_sys::{Element, HtmlElement, HtmlInputElement, HtmlTextAreaElement};
use yew::{
    html, initialize, run_loop, services::console::ConsoleService, App, Component, ComponentLink, Html, InputData,
};
use yew_mdc_widgets::{
    auto_init,
    utils::dom::{self, JsObjectAccess},
    Button, Card, CardContent, CustomEvent, Dialog, Fab, IconButton, ListItem, MdcWidget, Menu, TextField, TopAppBar,
};

struct FullNote {
    note: Note,
    is_modified: bool,
    is_new: bool,
}

impl FullNote {
    fn initial(note: Note) -> Self {
        Self {
            note,
            is_modified: false,
            is_new: false,
        }
    }

    fn new(note: Note) -> Self {
        Self {
            note,
            is_modified: true,
            is_new: true,
        }
    }

    fn note_mut(&mut self) -> &mut Note {
        self.is_modified = true;
        &mut self.note
    }

    fn is_modified(&self) -> bool {
        self.is_modified
    }

    fn is_new(&self) -> bool {
        self.is_new
    }
}

impl Deref for FullNote {
    type Target = Note;

    fn deref(&self) -> &Self::Target {
        let Self { note, .. } = self;
        note
    }
}

struct Root {
    link: ComponentLink<Self>,
    fetcher: JsonFetcher,
    notes: Vec<FullNote>,
    current_note_index: Option<usize>,
    current_mode: Option<Mode>,
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    View,
    Edit,
}

enum Msg {
    GetInitialNote(String),
    OpenNote(String, Mode),
    OpenCurrentNote(Mode),
    EditContent(String),
    Updated,
    SaveChanges,
    DiscardChanges,
    NewNote,
    RenameNote(String, String),
    DeleteNote(String),
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
            current_mode: None,
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
            },
            Msg::OpenNote(name, mode) => {
                self.current_mode.replace(mode);

                if let Some(index) = self.notes.iter().position(|note| note.name == name) {
                    if self.notes[index].is_modified() {
                        self.current_note_index = Some(index);
                        self.link.send_message(Msg::OpenCurrentNote(mode));
                        return false;
                    }
                }

                self.link.send_message(Msg::GetInitialNote(name));
                false
            },
            Msg::OpenCurrentNote(mode) => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    match mode {
                        Mode::View => {
                            dom::get_exist_element_by_id::<HtmlElement>("note-dialog__view").set_inner_html(
                                &to_view_inner_html(note.content.content().expect("Content should be present")),
                            );
                            show_element("note-dialog__view");
                            hide_element("note-dialog__edit");
                        },
                        Mode::Edit => {
                            dom::select_exist_element::<HtmlTextAreaElement>("#note-dialog__edit > textarea")
                                .set_value(note.content.content().expect("Content should be present"));
                            show_element("note-dialog__edit");
                            hide_element("note-dialog__view");
                        },
                    }

                    if note.is_modified() {
                        show_element("save-note-button");
                        show_element("discard-note-button");
                    } else {
                        hide_element("save-note-button");
                        hide_element("discard-note-button");
                    }

                    IconButton::set_on_by_id("edit_mode", mode == Mode::Edit);
                    Dialog::open_existing("note-dialog");
                }
                false
            },
            Msg::EditContent(content) => {
                let index = self.current_note_index.expect("Index should be presented");
                if !self.notes[index].is_modified() {
                    show_element("save-note-button");
                    show_element("discard-note-button");
                }
                self.notes[index].note_mut().content = NoteContent::FullBody(content);
                false
            },
            Msg::Updated => true,
            Msg::SaveChanges => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    if let Some(content) = note.content.content() {
                        let uri = format!("/notes/note/{}", note.name);
                        let body = content.to_string();
                        self.fetcher
                            .send_post(uri, body, JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error))
                            .context("Get note error")
                            .msg_error(&self.link);
                    } else {
                        self.link
                            .send_message(Msg::Error(anyhow!("Note content does not exist")));
                    }
                }
                false
            },
            Msg::DiscardChanges => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    if note.is_new() {
                        let index = self.current_note_index.unwrap();
                        self.notes.remove(index);
                        Dialog::close_existing("note-dialog");
                    } else {
                        self.link.send_message(Msg::GetInitialNote(note.name.clone()));
                    }
                }
                false
            },
            Msg::NewNote => {
                let name = dom::select_exist_element::<HtmlInputElement>("#new-note-name > input").value();

                if !self.notes.iter().any(|note| note.name == name) {
                    self.notes.push(FullNote::new(Note {
                        name: name.clone(),
                        content: NoteContent::FullBody(String::new()),
                    }));
                    self.notes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                    self.current_note_index = self.notes.iter().position(|note| note.name == name);
                    self.current_mode.replace(Mode::Edit);

                    Dialog::close_existing("add-note-dialog");
                    self.link.send_message(Msg::OpenCurrentNote(Mode::Edit));
                }
                false
            },
            Msg::RenameNote(name, new_name) => {
                let uri = format!("/notes/rename/{}", name);
                self.fetcher
                    .send_post(uri, new_name, JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error))
                    .context("Rename note error")
                    .msg_error(&self.link);
                false
            },
            Msg::DeleteNote(name) => {
                let uri = format!("/notes/delete/{}", name);
                self.fetcher
                    .send_post(uri, "", JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error))
                    .context("Delete note error")
                    .msg_error(&self.link);
                false
            },
            Msg::Fetch(Response::Notes(notes)) => {
                self.notes = notes.into_iter().map(FullNote::initial).collect();
                true
            },
            Msg::Fetch(Response::Note(note)) => {
                for (i, full_note) in self.notes.iter_mut().enumerate() {
                    if full_note.name == note.name {
                        *full_note = FullNote::initial(note);
                        self.current_note_index = Some(i);
                        break;
                    }
                }
                match self.current_mode {
                    Some(mode) => {
                        self.link.send_message(Msg::OpenCurrentNote(mode));
                        false
                    },
                    None => true,
                }
            },
            Msg::Fetch(Response::Error(err)) => {
                self.link.send_message(Msg::Error(anyhow!("{}", err)));
                false
            },
            Msg::Error(err) => {
                ConsoleService::error(&format!("{}", err));
                true
            },
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("Notes lapp example")
            .enable_shadow_when_scroll_window();

        let note_cards = self.notes.iter().map(|note| {
            let menu_id = format!("{}-menu", note.name);
            let menu = Menu::new()
                .id(&menu_id)
                .item(ListItem::new().text("Rename").on_click({
                    let note_name = note.name.clone();
                    move |_| {
                        let input = dom::select_exist_element::<HtmlInputElement>("#note-new-name > input");
                        input.set_value(&note_name);
                        input.dataset().set("note_name", &note_name).ok();
                        Dialog::open_existing("rename-note-dialog");
                    }
                }))
                .divider()
                .item(ListItem::new().text("Delete").on_click({
                    let note_name = note.name.clone();
                    move |_| {
                        dom::get_exist_element_by_id::<HtmlElement>("delete-note_name").set_inner_html(&note_name);
                        Dialog::open_existing("confirm-delete-note-dialog");
                    }
                }));

            let edit_button = IconButton::new()
                .class(CardContent::ACTION_ICON_CLASSES)
                .icon("edit")
                .on_click(self.link.callback({
                    let name = note.name.clone();
                    move |_| Msg::OpenNote(name.clone(), Mode::Edit)
                }));
            let menu_button = IconButton::new()
                .class(CardContent::ACTION_ICON_CLASSES)
                .icon("more_horiz")
                .on_click(move |_| Menu::open_existing(&menu_id));

            Card::new(&note.name)
                .content(CardContent::primary_action(html! {
                    <div class = "note-card__content" onclick = self.link.callback({
                        let name = note.name.clone();
                        move |_| Msg::OpenNote(name.clone(), Mode::View)
                    })>
                        { to_preview_html(&note.content) }
                    </div>
                }))
                .content(CardContent::actions().action_icons(html! { <>
                    { edit_button } <div class = Menu::ANCHOR_CLASS>{ menu_button } { menu }</div>
                </> }))
        });

        let view_note_dialog = self.view_note_dialog();
        let add_note_dialog = self.add_note_dialog();
        let confirm_delete_note_dialog = self.confirm_delete_note_dialog();
        let rename_note_dialog = self.rename_note_dialog();
        let add_note_button = Fab::new()
            .id("add-note-button")
            .icon("add")
            .on_click(|_| Dialog::open_existing("add-note-dialog"));

        html! {
            <div class = "app-content">
                { top_app_bar }
                <div class = "mdc-top-app-bar--fixed-adjust">
                    <div class = "content-container">
                        <h1 class = "title mdc-typography--headline5">{ "Notes" }</h1>

                        { view_note_dialog }
                        { add_note_dialog }
                        { confirm_delete_note_dialog }
                        { rename_note_dialog }

                        <div class = "notes mdc-layout-grid">
                            <div class = "mdc-layout-grid__inner">
                                { for note_cards.map(|card| html! { <div class = "mdc-layout-grid__cell">{ card }</div> }) }
                            </div>
                        </div>

                        { add_note_button }
                    </div>
                </div>
            </div>
        }
    }

    fn rendered(&mut self, _first_render: bool) {
        auto_init();
    }
}

impl Root {
    fn view_note_dialog(&self) -> Html {
        let switch_mode_button = IconButton::new()
            .id("edit_mode")
            .class(CardContent::ACTION_ICON_CLASSES)
            .toggle("visibility", "edit")
            .on_change(self.link.callback(|event: CustomEvent| {
                if event.detail().get("isOn").as_bool().unwrap_or(false) {
                    Msg::OpenCurrentNote(Mode::Edit)
                } else {
                    Msg::OpenCurrentNote(Mode::View)
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
                    .on_click(self.link.callback(|_| Msg::SaveChanges)),
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

    fn add_note_dialog(&self) -> Html {
        Dialog::new()
            .id("add-note-dialog")
            .content_item(TextField::filled().id("new-note-name").label("Note name"))
            .action(
                Button::new()
                    .id("save-note-button")
                    .label("Add")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(self.link.callback(|_| Msg::NewNote)),
            )
            .action(
                Button::new()
                    .id("discard-note-button")
                    .label("Cancel")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("add-note-dialog")),
            )
            .on_closed(self.link.callback(|_| Msg::Updated))
            .into()
    }

    fn confirm_delete_note_dialog(&self) -> Html {
        Dialog::new()
            .id("confirm-delete-note-dialog")
            .title(html! { <h2> { "Delete confirmation" } </h2> })
            .content_item(html! {
                <span> { "Do you want to delete \"" } <span id = "delete-note_name" /> { "\" note?" } </span>
            })
            .action(
                Button::new()
                    .label("Yes")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(self.link.callback(|_| {
                        let name = dom::get_exist_element_by_id::<HtmlElement>("delete-note_name").inner_html();
                        Dialog::close_existing("confirm-delete-note-dialog");
                        Msg::DeleteNote(name)
                    })),
            )
            .action(
                Button::new()
                    .label("No")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("confirm-delete-note-dialog")),
            )
            .into()
    }

    fn rename_note_dialog(&self) -> Html {
        Dialog::new()
            .id("rename-note-dialog")
            .content_item(TextField::filled().id("note-new-name").label("Note name"))
            .action(
                Button::new()
                    .id("rename-note-button")
                    .label("Rename")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(self.link.callback(|_| {
                        let input = dom::select_exist_element::<HtmlInputElement>("#note-new-name > input");

                        if let Some(name) = input.dataset().get("note_name") {
                            let new_name = input.value();
                            Dialog::close_existing("rename-note-dialog");
                            Msg::RenameNote(name, new_name)
                        } else {
                            Msg::Error(anyhow!("Old note name not found"))
                        }
                    })),
            )
            .action(
                Button::new()
                    .label("Cancel")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("rename-note-dialog")),
            )
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

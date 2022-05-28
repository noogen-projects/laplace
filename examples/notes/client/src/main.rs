#![recursion_limit = "256"]

use std::ops::Deref;

use anyhow::{anyhow, Error};
use laplace_yew::RawHtml;
use lew::SimpleEditor;
use notes_common::{Note, NoteContent, Response};
use pulldown_cmark::{html as cmark_html, Options, Parser};
use wasm_web_helpers::{
    error::Result,
    fetch::{JsonFetcher, Response as WebResponse},
};
use web_sys::{Element, HtmlElement, HtmlInputElement, HtmlTextAreaElement};
use yew::{html, Callback, Component, Context, Html, InputEvent};
use yew_mdc_widgets::{
    auto_init, console,
    dom::{self, existing::JsObjectAccess},
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
    EditContent,
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

    fn create(ctx: &Context<Self>) -> Self {
        JsonFetcher::send_get("/notes/list", {
            let callback = callback(ctx);
            move |response_result| callback.emit(response_result)
        });

        Self {
            notes: Vec::new(),
            current_note_index: None,
            current_mode: None,
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::GetInitialNote(name) => {
                JsonFetcher::send_get(format!("/notes/note/{}", name), {
                    let callback = callback(ctx);
                    move |response_result| callback.emit(response_result)
                });
                false
            },
            Msg::OpenNote(name, mode) => {
                self.current_mode.replace(mode);

                if let Some(index) = self.notes.iter().position(|note| note.name == name) {
                    if self.notes[index].is_modified() {
                        self.current_note_index = Some(index);
                        ctx.link().send_message(Msg::OpenCurrentNote(mode));
                        return false;
                    }
                }

                ctx.link().send_message(Msg::GetInitialNote(name));
                false
            },
            Msg::OpenCurrentNote(mode) => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    match mode {
                        Mode::View => {
                            dom::existing::get_element_by_id::<HtmlElement>("note-dialog__view").set_inner_html(
                                &to_view_inner_html(note.content.content().expect("Content should be present")),
                            );
                            show_element("note-dialog__view");
                            hide_element("note-dialog__edit");
                        },
                        Mode::Edit => {
                            dom::existing::select_element::<HtmlTextAreaElement>("#note-dialog__edit > textarea")
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
            Msg::EditContent => {
                let index = self.current_note_index.expect("Index should be presented");
                if !self.notes[index].is_modified() {
                    show_element("save-note-button");
                    show_element("discard-note-button");
                }
                let content =
                    dom::existing::select_element::<HtmlTextAreaElement>("#note-dialog__edit > textarea").value();
                self.notes[index].note_mut().content = NoteContent::FullBody(content);
                false
            },
            Msg::Updated => true,
            Msg::SaveChanges => {
                if let Some(note) = self.current_note_index.map(|index| &self.notes[index]) {
                    if let Some(content) = note.content.content() {
                        let uri = format!("/notes/note/{}", note.name);
                        let body = content.to_string();
                        JsonFetcher::send_post(uri, body, {
                            let callback = callback(ctx);
                            move |response_result| callback.emit(response_result)
                        });
                    } else {
                        ctx.link()
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
                        ctx.link().send_message(Msg::GetInitialNote(note.name.clone()));
                    }
                }
                false
            },
            Msg::NewNote => {
                let name = dom::existing::select_element::<HtmlInputElement>("#new-note-name > input").value();

                if !self.notes.iter().any(|note| note.name == name) {
                    self.notes.push(FullNote::new(Note {
                        name: name.clone(),
                        content: NoteContent::FullBody(String::new()),
                    }));
                    self.notes.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                    self.current_note_index = self.notes.iter().position(|note| note.name == name);
                    self.current_mode.replace(Mode::Edit);

                    Dialog::close_existing("add-note-dialog");
                    ctx.link().send_message(Msg::OpenCurrentNote(Mode::Edit));
                }
                false
            },
            Msg::RenameNote(name, new_name) => {
                let uri = format!("/notes/rename/{}", name);
                JsonFetcher::send_post(uri, new_name, {
                    let callback = callback(ctx);
                    move |response_result| callback.emit(response_result)
                });
                false
            },
            Msg::DeleteNote(name) => {
                let uri = format!("/notes/delete/{}", name);
                JsonFetcher::send_post(uri, "", {
                    let callback = callback(ctx);
                    move |response_result| callback.emit(response_result)
                });
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
                        ctx.link().send_message(Msg::OpenCurrentNote(mode));
                        false
                    },
                    None => true,
                }
            },
            Msg::Fetch(Response::Error(err)) => {
                ctx.link().send_message(Msg::Error(anyhow!("{}", err)));
                false
            },
            Msg::Error(err) => {
                console::error!(&format!("{}", err));
                true
            },
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
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
                        let input = dom::existing::select_element::<HtmlInputElement>("#note-new-name > input");
                        input.set_value(&note_name);
                        input.dataset().set("note_name", &note_name).ok();
                        Dialog::open_existing("rename-note-dialog");
                    }
                }))
                .divider()
                .item(ListItem::new().text("Delete").on_click({
                    let note_name = note.name.clone();
                    move |_| {
                        dom::existing::get_element_by_id::<HtmlElement>("delete-note_name").set_inner_html(&note_name);
                        Dialog::open_existing("confirm-delete-note-dialog");
                    }
                }));

            let edit_button = IconButton::new()
                .class(CardContent::ACTION_ICON_CLASSES)
                .icon("edit")
                .on_click(ctx.link().callback({
                    let name = note.name.clone();
                    move |_| Msg::OpenNote(name.clone(), Mode::Edit)
                }));
            let menu_button = IconButton::new()
                .class(CardContent::ACTION_ICON_CLASSES)
                .icon("more_horiz")
                .on_click(move |_| Menu::open_existing(&menu_id));

            Card::new(&note.name)
                .content(CardContent::primary_action(html! {
                    <div class = "note-card__content" onclick = { ctx.link().callback({
                        let name = note.name.clone();
                        move |_| Msg::OpenNote(name.clone(), Mode::View)
                    }) } >
                        { to_preview_html(&note.content) }
                    </div>
                }))
                .content(CardContent::actions().action_icons(html! { <>
                    { edit_button } <div class = { Menu::ANCHOR_CLASS }>{ menu_button } { menu }</div>
                </> }))
        });

        let view_note_dialog = self.view_note_dialog(ctx);
        let add_note_dialog = self.add_note_dialog(ctx);
        let confirm_delete_note_dialog = self.confirm_delete_note_dialog(ctx);
        let rename_note_dialog = self.rename_note_dialog(ctx);
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

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        auto_init();
    }
}

impl Root {
    fn view_note_dialog(&self, ctx: &Context<Self>) -> Html {
        let switch_mode_button = IconButton::new()
            .id("edit_mode")
            .class(CardContent::ACTION_ICON_CLASSES)
            .toggle("visibility", "edit")
            .on_change(ctx.link().callback(|event: CustomEvent| {
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
                            cols = 40 oninput = { ctx.link().callback(|_: InputEvent| Msg::EditContent) } />
                </>
            })
            .action(
                Button::new()
                    .id("save-note-button")
                    .class("hidden")
                    .label("Save")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(ctx.link().callback(|_| Msg::SaveChanges)),
            )
            .action(
                Button::new()
                    .id("discard-note-button")
                    .class("hidden")
                    .label("Discard")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(ctx.link().callback(|_| Msg::DiscardChanges)),
            )
            .on_closed(ctx.link().callback(|_| Msg::Updated))
            .into()
    }

    fn add_note_dialog(&self, ctx: &Context<Self>) -> Html {
        Dialog::new()
            .id("add-note-dialog")
            .content_item(TextField::filled().id("new-note-name").label("Note name"))
            .action(
                Button::new()
                    .id("save-note-button")
                    .label("Add")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(ctx.link().callback(|_| Msg::NewNote)),
            )
            .action(
                Button::new()
                    .id("discard-note-button")
                    .label("Cancel")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("add-note-dialog")),
            )
            .on_closed(ctx.link().callback(|_| Msg::Updated))
            .into()
    }

    fn confirm_delete_note_dialog(&self, ctx: &Context<Self>) -> Html {
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
                    .on_click(ctx.link().callback(|_| {
                        let name = dom::existing::get_element_by_id::<HtmlElement>("delete-note_name").inner_html();
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

    fn rename_note_dialog(&self, ctx: &Context<Self>) -> Html {
        Dialog::new()
            .id("rename-note-dialog")
            .content_item(TextField::filled().id("note-new-name").label("Note name"))
            .action(
                Button::new()
                    .id("rename-note-button")
                    .label("Rename")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(ctx.link().callback(|_| {
                        let input = dom::existing::select_element::<HtmlInputElement>("#note-new-name > input");

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
    html! { <RawHtml inner_html = { to_view_inner_html(&preview) } /> }
}

fn new_cmark_parser(source: &str) -> Parser {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    Parser::new_ext(source, options)
}

fn show_element(id: impl AsRef<str>) {
    dom::existing::get_element_by_id::<Element>(id.as_ref())
        .class_list()
        .remove_1("hidden")
        .ok();
}

fn hide_element(id: impl AsRef<str>) {
    let element = dom::existing::get_element_by_id::<Element>(id.as_ref());
    if !element.class_list().contains("hidden") {
        element.class_list().add_1("hidden").ok();
    }
}

fn callback(ctx: &Context<Root>) -> Callback<Result<(WebResponse, Result<Response>)>> {
    ctx.link()
        .callback(|response_result: Result<(WebResponse, Result<Response>)>| {
            response_result
                .map(|(response, body)| {
                    body.map(Msg::Fetch).unwrap_or_else(|err| {
                        Msg::Error(anyhow!(
                            "Parse response body error: {:?}, for request {}",
                            err,
                            response.url(),
                        ))
                    })
                })
                .unwrap_or_else(|err| Msg::Error(err.into()))
        })
}

fn main() {
    let root = dom::existing::get_element_by_id("root");
    yew::Renderer::<Root>::with_root(root).render();
}

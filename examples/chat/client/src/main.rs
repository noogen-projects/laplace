#![recursion_limit = "512"]

use std::cell::RefCell;

use anyhow::{anyhow, Context, Error};
use chat_common::{Peer, WsMessage, WsResponse};
use dapla_yew::{JsonFetcher, MsgError, RawHtml, StringResponse};
use libp2p_core::{identity::ed25519::Keypair, PeerId, PublicKey};
use pulldown_cmark::{html as cmark_html, Options, Parser};
use web_sys::{HtmlElement, HtmlInputElement, HtmlTextAreaElement};
use yew::{
    classes,
    format::Json,
    html, initialize, run_loop,
    services::{
        console::ConsoleService,
        websocket::{WebSocketService, WebSocketStatus, WebSocketTask},
    },
    App, Component, ComponentLink, Html, InputData, KeyboardEvent, MouseEvent,
};
use yew_mdc_widgets::{
    auto_init, drawer,
    utils::dom::{self, JsObjectAccess},
    Button, Dialog, Drawer, Element, IconButton, List, ListItem, MdcWidget, TextField, TopAppBar,
};

enum State {
    SignIn,
    Chat(Chat),
}

struct Chat {
    keys: Keys,
    peer_id: PeerId,
    resize_data: ResizeData,
    ws: WebSocketTask,
    channels: Vec<Channel>,
    active_channel_idx: usize,
}

struct Keys {
    keypair: Keypair,
    public_key: String,
    secret_key: String,
}

#[derive(Default)]
struct ResizeDir {
    start_cursor_screen_pos: i32,
    start_size: i32,
    tracking: bool,
}

#[derive(Default)]
struct ResizeData {
    width: ResizeDir,
    height: ResizeDir,
}

struct Message {
    is_mine: bool,
    body: String,
}

struct Channel {
    correspondent_id: String,
    correspondent_name: String,
    thread: Vec<Message>,
}

struct Root {
    link: ComponentLink<Self>,
    fetcher: JsonFetcher,
    state: State,
}

enum WsAction {
    SendData(String),
    ReceiveData(WsResponse),
    Lost,
}

enum Msg {
    SignIn,
    InitChat { keys: Keys, peer_id: PeerId },
    ChatScreenMouseMove(MouseEvent),
    ToggleChatSidebarSplitHandle(MouseEvent),
    ToggleChatEditorSplitHandle(MouseEvent),
    AddPeer(String),
    SwitchChannel(usize),
    Ws(WsAction),
    Error(Error),
    None,
}

impl From<WsAction> for Msg {
    fn from(action: WsAction) -> Self {
        Self::Ws(action)
    }
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
        Self {
            link,
            fetcher: JsonFetcher::new(),
            state: State::SignIn,
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::SignIn => {
                let public_key = TextField::value("public-key");
                let secret_key = TextField::value("secret-key");

                if let Ok(keypair) = (|| {
                    let mut bytes = bs58::decode(&secret_key)
                        .into_vec()
                        .context("Decode secret key error")?;
                    bytes.extend_from_slice(
                        &bs58::decode(&public_key)
                            .into_vec()
                            .context("Decode public key error")?,
                    );
                    Keypair::decode(&mut bytes).context("Decode keypair error")
                })()
                .msg_error_map(&self.link)
                {
                    let peer_id = PeerId::from(PublicKey::Ed25519(keypair.public()));
                    let body = serde_json::to_string(&Peer {
                        peer_id: peer_id.to_bytes(),
                        keypair: keypair.encode().into(),
                    })
                    .expect("Peer should be serialize to JSON");

                    let success_msg = RefCell::new(Some(Msg::InitChat {
                        keys: Keys {
                            keypair,
                            public_key,
                            secret_key,
                        },
                        peer_id,
                    }));

                    self.fetcher
                        .send_post_json(
                            "/chat/p2p",
                            body,
                            self.link.callback(move |response: StringResponse| {
                                if response.status().is_success() {
                                    success_msg
                                        .borrow_mut()
                                        .take()
                                        .unwrap_or_else(|| Msg::Error(anyhow!("Multiple success fetch received")))
                                } else {
                                    Msg::Error(anyhow!(
                                        "Fetch status: {:?}, body: {:?}",
                                        response.status(),
                                        response.into_body(),
                                    ))
                                }
                            }),
                        )
                        .context("Start P2P error")
                        .msg_error(&self.link);
                }
                true
            },
            Msg::InitChat { keys, peer_id } => {
                let location = dom::document().location().expect("Location should be existing");
                let url = format!("ws://{}/chat/ws", location.host().expect("Location host expected"));
                let callback = self.link.callback(|Json(response)| match response {
                    Ok(data) => Msg::Ws(WsAction::ReceiveData(data)),
                    Err(err) => Msg::Error(err),
                });
                let notification = self.link.batch_callback(|status| match status {
                    WebSocketStatus::Opened => vec![],
                    WebSocketStatus::Closed | WebSocketStatus::Error => vec![WsAction::Lost.into()],
                });
                let ws = WebSocketService::connect(&url, callback, notification)
                    .unwrap_or_else(|err| panic!("WS should be created for URL {}: {:?}", url, err));

                self.state = State::Chat(Chat {
                    keys,
                    peer_id,
                    resize_data: ResizeData::default(),
                    ws,
                    channels: Default::default(),
                    active_channel_idx: 0,
                });
                true
            },
            Msg::ChatScreenMouseMove(event) => {
                if let State::Chat(Chat {
                    ref mut resize_data, ..
                }) = self.state
                {
                    if resize_data.width.tracking && event.buttons() == 1 {
                        let delta_x = event.screen_x() - resize_data.width.start_cursor_screen_pos;
                        let container = select_exist_html_element(".chat-screen");
                        let width =
                            100.max((resize_data.width.start_size + delta_x).min(container.client_width() - 400));
                        set_exist_element_style(".chat-sidebar", "width", &format!("{}px", width));
                    } else if resize_data.height.tracking && event.buttons() == 1 {
                        let delta_y = event.screen_y() - resize_data.height.start_cursor_screen_pos;
                        let container = select_exist_html_element(".chat-screen");
                        let height =
                            72.max((resize_data.height.start_size - delta_y).min(container.client_height() - 100));
                        set_exist_element_style(".chat-editor textarea", "height", &format!("{}px", height));
                    } else {
                        resize_data.width.tracking = false;
                        resize_data.height.tracking = false;
                        remove_class_from_exist_html_element(".chat-screen", "resize-hor-cursor");
                        remove_class_from_exist_html_element(".chat-screen", "resize-ver-cursor");
                    }
                }
                false
            },
            Msg::ToggleChatSidebarSplitHandle(event) => {
                if let State::Chat(Chat {
                    ref mut resize_data, ..
                }) = self.state
                {
                    if event.button() == 0 {
                        let sidebar = select_exist_html_element(".chat-sidebar");
                        *resize_data = ResizeData {
                            width: ResizeDir {
                                start_cursor_screen_pos: event.screen_x(),
                                start_size: sidebar.client_width(),
                                tracking: true,
                            },
                            ..Default::default()
                        };
                        add_class_to_exist_html_element(".chat-screen", "resize-hor-cursor");
                    }
                }
                false
            },
            Msg::ToggleChatEditorSplitHandle(event) => {
                if let State::Chat(Chat {
                    ref mut resize_data, ..
                }) = self.state
                {
                    if event.button() == 0 {
                        let editor = select_exist_html_element(".chat-editor textarea");
                        *resize_data = ResizeData {
                            height: ResizeDir {
                                start_cursor_screen_pos: event.screen_y(),
                                start_size: editor.client_height(),
                                tracking: true,
                            },
                            ..Default::default()
                        };
                        add_class_to_exist_html_element(".chat-screen", "resize-ver-cursor");
                    }
                }
                false
            },
            Msg::AddPeer(peer_id) => {
                if let State::Chat(state) = &mut self.state {
                    state.channels.push(Channel {
                        correspondent_id: peer_id.clone(),
                        correspondent_name: "<Unnamed>".to_string(),
                        thread: vec![],
                    });
                    state.ws.send(Json(&WsMessage::AddPeer(peer_id)));
                    true
                } else {
                    false
                }
            },
            Msg::SwitchChannel(idx) => {
                if let State::Chat(state) = &mut self.state {
                    if state.active_channel_idx != idx {
                        state.active_channel_idx = idx;
                        return true;
                    }
                }
                false
            },
            Msg::Ws(action) => match action {
                WsAction::SendData(request) => {
                    if let State::Chat(state) = &mut self.state {
                        if let Some(channel) = state.channels.get_mut(state.active_channel_idx) {
                            channel.thread.push(Message {
                                is_mine: true,
                                body: request.clone(),
                            });
                            state.ws.send(Json(&WsMessage::Text {
                                peer_id: channel.correspondent_id.clone(),
                                msg: request,
                            }));
                        }
                    }
                    true
                },
                WsAction::ReceiveData(response) => {
                    match response {
                        WsResponse::Success(WsMessage::Text { peer_id, msg }) => {
                            if let State::Chat(state) = &mut self.state {
                                if let Some(channel) = state
                                    .channels
                                    .iter_mut()
                                    .find(|channel| channel.correspondent_id == peer_id)
                                {
                                    channel.thread.push(Message {
                                        is_mine: false,
                                        body: msg,
                                    });
                                    return true;
                                }
                            }
                        },
                        msg => self.link.send_message(Msg::Error(anyhow!("{:?}", msg))),
                    }
                    false
                },
                WsAction::Lost => true,
            },
            Msg::Error(err) => {
                ConsoleService::error(&format!("{}", err));
                true
            },
            Msg::None => false,
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("Chat dap")
            .navigation_item(IconButton::new().icon("menu"))
            .on_navigation(|_| {
                let drawer = dom::select_exist_element::<Element>("#chat-drawer").get(drawer::mdc::TYPE_NAME);
                let opened = drawer.get("open").as_bool().unwrap_or(false);
                drawer.set("open", !opened);
            });
        let mut drawer = Drawer::new()
            .modal()
            .id("chat-drawer")
            .title(html! { <h2 tabindex = 0>{ "Settings" }</h2> });
        let mut dialogs = html! {};

        let content = match &self.state {
            State::SignIn => self.view_sign_in(),
            State::Chat(state) => {
                drawer = drawer
                    .title(html! { <h3 contenteditable = "true">{ "User" }</h3> })
                    .content(
                        List::ul()
                            .divider()
                            .item(
                                ListItem::new()
                                    .icon("perm_identity")
                                    .text("Peer")
                                    .attr("tabindex", "0")
                                    .on_click(|_| Dialog::open_existing("peer-dialog")),
                            )
                            .item(
                                ListItem::new()
                                    .icon("vpn_key")
                                    .text("Keys")
                                    .on_click(|_| Dialog::open_existing("keys-dialog")),
                            )
                            .markup_only(),
                    );

                let peer_dialog = Dialog::new()
                    .id("peer-dialog")
                    .title(html! { <h2 tabindex = 0> { "Peer" } </h2> })
                    .content(html! { <div><strong>{ "ID: " }</strong> { state.peer_id.to_base58() }</div>});

                let keys_dialog = Dialog::new()
                    .id("keys-dialog")
                    .title(html! { <h2 tabindex = 0> { "Keys" } </h2> })
                    .content(
                        List::ul()
                            .item(html! { <div><strong>{ "Public: " }</strong> { &state.keys.public_key }</div> })
                            .item(html! { <div><strong>{ "Secret: " }</strong> { &state.keys.secret_key }</div> }),
                    );

                dialogs = html! {
                    <>
                        { peer_dialog }
                        { keys_dialog }
                    </>
                };

                self.view_chat(state)
            },
        };

        html! {
            <>
                { drawer }
                <div class = "mdc-drawer-scrim"></div>
                { dialogs }

                <div class = classes!("app-content", Drawer::APP_CONTENT_CLASS)>
                    { top_app_bar }
                    <div class = "mdc-top-app-bar--fixed-adjust content-container">
                        { content }
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
    fn view_sign_in(&self) -> Html {
        let generate_keypair_button = Button::new().id("generate-key-button").label("Generate").on_click(|_| {
            let keypair = Keypair::generate();
            let public_key = bs58::encode(keypair.public().encode()).into_string();
            let secret_key = bs58::encode(keypair.secret()).into_string();

            TextField::set_value("public-key", &public_key);
            TextField::set_value("secret-key", &secret_key);

            let sign_in_button = dom::get_exist_element_by_id::<HtmlElement>("sign-in-button");
            sign_in_button.remove_attribute("disabled").ok();
            sign_in_button.focus().ok();
            dom::get_exist_element_by_id::<HtmlElement>("generate-key-button")
                .set_attribute("disabled", "")
                .ok();
        });

        let sign_in_button = Button::new()
            .id("sign-in-button")
            .label("Sign In")
            .disabled()
            .on_click(self.link.callback(|_| Msg::SignIn));

        let sign_in_form = List::simple_ul().items(vec![
            ListItem::simple().child(html! {
                <span class = "mdc-typography--overline">{ "Enter or generate a keypair" }</span>
            }),
            ListItem::simple().child(
                TextField::outlined()
                    .id("public-key")
                    .class("expand")
                    .label("Public key")
                    .on_input(|input: InputData| {
                        let generate_key_button = dom::get_exist_element_by_id::<HtmlElement>("generate-key-button");
                        let sign_in_button = dom::get_exist_element_by_id::<HtmlElement>("sign-in-button");

                        if input.value.is_empty() && TextField::value("secret-key").is_empty() {
                            generate_key_button.remove_attribute("disabled").ok();
                            sign_in_button.set_attribute("disabled", "").ok();
                        } else if generate_key_button.get_attribute("disabled").is_none() {
                            generate_key_button.set_attribute("disabled", "").ok();
                            sign_in_button.remove_attribute("disabled").ok();
                        }
                    }),
            ),
            ListItem::simple().child(
                TextField::outlined()
                    .id("secret-key")
                    .class("expand")
                    .label("Secret key")
                    .on_input(|input: InputData| {
                        let generate_key_button = dom::get_exist_element_by_id::<HtmlElement>("generate-key-button");
                        let sign_in_button = dom::get_exist_element_by_id::<HtmlElement>("sign-in-button");

                        if input.value.is_empty() && TextField::value("public-key").is_empty() {
                            generate_key_button.remove_attribute("disabled").ok();
                            sign_in_button.set_attribute("disabled", "").ok();
                        } else if generate_key_button.get_attribute("disabled").is_none() {
                            generate_key_button.set_attribute("disabled", "").ok();
                            sign_in_button.remove_attribute("disabled").ok();
                        }
                    }),
            ),
            ListItem::simple().child(html! {
                <div class = "sign-in-actions">
                    { generate_keypair_button }
                    { sign_in_button }
                </div>
            }),
        ]);

        html! {
            <div class = "keys-form">
                { sign_in_form }
            </div>
        }
    }

    fn view_chat(&self, state: &Chat) -> Html {
        let mut channels = List::nav().two_line().divider();
        let mut messages = html! {};
        for (idx, channel) in state.channels.iter().enumerate() {
            let mut item = ListItem::link(format!("#{}", channel.correspondent_id))
                .icon("person")
                .text(&channel.correspondent_name)
                .text(&channel.correspondent_id)
                .on_click(self.link.callback(move |_| Msg::SwitchChannel(idx)));

            if idx == state.active_channel_idx {
                item = item.selected(true).attr("tabindex", "0");
                messages = html! {
                    { for channel.thread.iter().map(|msg| html! { <div><RawHtml inner_html = to_view_inner_html(&msg.body) /></div> }) }
                };
            }
            channels = channels.item(item).divider()
        }
        channels = channels.markup_only();

        let add_peer_dialog = self.view_add_peer_dialog();
        let add_peer_button = IconButton::new()
            .icon("add")
            .class("centered-hor")
            .on_click(|_| Dialog::open_existing("add-peer-dialog"));

        let sender = self.link.callback(|event: KeyboardEvent| {
            if event.key() == "Enter" && event.ctrl_key() {
                let editor = dom::get_exist_element_by_id::<HtmlTextAreaElement>("editor");
                let message = editor.value();
                editor.set_value("");

                Msg::Ws(WsAction::SendData(message))
            } else {
                Msg::None
            }
        });
        let editor = html! {
            <label class = "mdc-text-field mdc-text-field--textarea mdc-text-field--no-label">
                <textarea id = "editor" class = "mdc-text-field__input" rows = "3" aria-label = "Label" onkeypress = sender></textarea>
            </label>
        };

        html! {
            <div class = "chat-screen" onmousemove = self.link.callback(|event| Msg::ChatScreenMouseMove(event))>
                <aside class = "chat-sidebar">
                    <div class = "chat-flex-container scrollable-content">
                        { channels }
                        { add_peer_button }
                        { add_peer_dialog }
                    </div>
                </aside>
                <div class = "chat-sidebar-split-handle resize-hor-cursor" onmousedown = self.link.callback(|event| {
                    Msg::ToggleChatSidebarSplitHandle(event)
                })></div>
                <div class = "chat-main">
                    <div class = "chat-flex-container">
                        <div id = "messages" class = "chat-messages">
                            { messages }
                        </div>
                        <div class = "chat-editor-split-handle resize-ver-cursor" onmousedown = self.link.callback(|event| {
                            Msg::ToggleChatEditorSplitHandle(event)
                        })></div>
                        <div class = "chat-editor">
                            { editor }
                        </div>
                    </div>
                </div>
            </div>
        }
    }

    fn view_add_peer_dialog(&self) -> Html {
        Dialog::new()
            .id("add-peer-dialog")
            .content_item(
                TextField::outlined()
                    .id("new-peer-id")
                    .class("keys-form")
                    .label("Peer ID"),
            )
            .action(
                Button::new()
                    .id("add-peer-button")
                    .label("Add")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(self.link.callback(|_| {
                        let id = dom::select_exist_element::<HtmlInputElement>("#new-peer-id > input").value();
                        Dialog::close_existing("add-peer-dialog");
                        Msg::AddPeer(id)
                    })),
            )
            .action(
                Button::new()
                    .label("Cancel")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("add-peer-dialog")),
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

fn new_cmark_parser(source: &str) -> Parser {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    Parser::new_ext(source, options)
}

pub fn select_exist_html_element(selector: &str) -> HtmlElement {
    dom::select_exist_element::<HtmlElement>(selector)
}

pub fn set_element_style(element: impl AsRef<HtmlElement>, property: &str, value: &str) {
    element
        .as_ref()
        .style()
        .set_property(property, value)
        .unwrap_or_else(|err| panic!("Can't set style \"{}:{}\": {:?}", property, value, err));
}

pub fn set_exist_element_style(selector: &str, property: &str, value: &str) {
    set_element_style(select_exist_html_element(selector), property, value);
}

pub fn add_class_to_html_element(element: impl AsRef<HtmlElement>, class: &str) {
    let class_name = element.as_ref().class_name();
    let mut exist_classes: Vec<_> = class_name.split_whitespace().collect();
    if !exist_classes.contains(&class) {
        exist_classes.push(class);
        element.as_ref().set_class_name(&exist_classes.join(" "));
    }
}

pub fn remove_class_from_html_element(element: impl AsRef<HtmlElement>, class: &str) {
    let class_name = element.as_ref().class_name();
    let mut exist_classes: Vec<_> = class_name.split_whitespace().collect();
    if let Some(index) = exist_classes.iter().position(|item| *item == class) {
        exist_classes.remove(index);
        element.as_ref().set_class_name(&exist_classes.join(" "));
    }
}

pub fn add_class_to_exist_html_element(selector: &str, class: &str) {
    add_class_to_html_element(select_exist_html_element(selector), class);
}

pub fn remove_class_from_exist_html_element(selector: &str, class: &str) {
    remove_class_from_html_element(select_exist_html_element(selector), class);
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

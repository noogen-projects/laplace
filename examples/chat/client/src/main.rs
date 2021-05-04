#![recursion_limit = "512"]

use anyhow::{anyhow, Context, Error};
use dapla_yew::{JsonFetcher, MsgError, RawHtml};
use libp2p_core::{identity::ed25519::Keypair, PeerId, PublicKey};
use web_sys::HtmlElement;
use yew::{
    html, initialize, run_loop, services::console::ConsoleService, App, Component, ComponentLink, Html, InputData,
    MouseEvent,
};
use yew_mdc_widgets::{auto_init, utils::dom, Button, List, ListItem, MdcWidget, TextField, TopAppBar};

enum Screen {
    SignIn,
    Chat(ChatState),
}

struct ChatState {
    keys: Keys,
    peer_id: PeerId,
    resize_data: ResizeData,
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

struct Root {
    link: ComponentLink<Self>,
    screen: Screen,
}

enum Msg {
    SignIn,
    ChatScreenMouseMove(MouseEvent),
    ToggleChatSidebarSplitHandle(MouseEvent),
    ToggleChatEditorSplitHandle(MouseEvent),
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
        Self {
            link,
            screen: Screen::SignIn,
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
                    self.screen = Screen::Chat(ChatState {
                        keys: Keys {
                            keypair,
                            public_key,
                            secret_key,
                        },
                        peer_id,
                        resize_data: ResizeData::default(),
                    });
                }
                true
            }
            Msg::ChatScreenMouseMove(event) => {
                if let Screen::Chat(ChatState {
                    ref mut resize_data, ..
                }) = self.screen
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
            }
            Msg::ToggleChatSidebarSplitHandle(event) => {
                if let Screen::Chat(ChatState {
                    ref mut resize_data, ..
                }) = self.screen
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
            }
            Msg::ToggleChatEditorSplitHandle(event) => {
                if let Screen::Chat(ChatState {
                    ref mut resize_data, ..
                }) = self.screen
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
        let top_app_bar = TopAppBar::new().id("top-app-bar").title("Chat dap");

        let content = match &self.screen {
            Screen::SignIn => self.view_sign_in(),
            Screen::Chat(state) => self.view_chat(state),
        };

        html! {
            <>
                <div class = "app-content">
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
            <div class = "sign-in-form">
                { sign_in_form }
            </div>
        }
    }

    fn view_chat(&self, state: &ChatState) -> Html {
        let channels = List::nav()
            .divider()
            .item(
                ListItem::link("#added")
                    .icon("done")
                    .text("Added")
                    .attr("tabindex", "0"),
            )
            .divider()
            .item(ListItem::link("#modified").icon("edit").text("Modified"))
            .divider()
            .item(ListItem::link("#viewed").icon("visibility").text("Viewed"))
            .divider()
            .markup_only();

        let messages = html! {
            <>
                <div>{ "Peer ID: " } { &state.peer_id }</div>
                <div>{ "Public: " } { &state.keys.public_key }</div>
                <div>{ "Secret: " } { &state.keys.secret_key }</div>
            </>
        };

        let editor = html! {
            <label class = "mdc-text-field mdc-text-field--textarea mdc-text-field--no-label">
                <textarea class = "mdc-text-field__input" rows = "3" aria-label = "Label"></textarea>
            </label>
        };

        html! {
            <div class = "chat-screen" onmousemove = self.link.callback(|event| Msg::ChatScreenMouseMove(event))>
                <aside class = "chat-sidebar">
                    <div class = "chat-flex-container scrollable-content">
                        { channels }
                    </div>
                </aside>
                <div class = "chat-sidebar-split-handle resize-hor-cursor" onmousedown = self.link.callback(|event| {
                    Msg::ToggleChatSidebarSplitHandle(event)
                })></div>
                <div class = "chat-main">
                    <div class = "chat-flex-container">
                        <div class = "chat-messages">
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

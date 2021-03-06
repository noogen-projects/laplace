#![recursion_limit = "256"]

use anyhow::Error;
use libp2p_core::identity::ed25519::Keypair;
use web_sys::HtmlElement;
use yew::{
    html, initialize, run_loop, services::console::ConsoleService, App, Component, ComponentLink, Html, InputData,
};
use yew_mdc_widgets::{auto_init, utils::dom, Button, List, ListItem, MdcWidget, TextField, TopAppBar};

struct Root {
    link: ComponentLink<Self>,
}

enum Msg {
    SignIn,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { link }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::SignIn => false,
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("Chat dap")
            .enable_shadow_when_scroll_window();

        html! {
            <>
                <div class = "app-content">
                    { top_app_bar }
                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            { self.view_sign_in() }
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
                <span class = "mdc-typography--overline">{ "Enter or generate keypair" }</span>
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

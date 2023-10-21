use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;

use anyhow::{anyhow, Context as _, Error};
use laplace_common::api::{Response as CommonLappResponse, UpdateQuery};
use laplace_common::lapp::{Lapp as CommonLapp, LappSettings, Permission};
use laplace_yew::error::MsgError;
use wasm_web_helpers::error::Result;
use wasm_web_helpers::fetch::{JsonFetcher, Response};
use web_sys::{FormData, HtmlInputElement};
use yew::{self, classes, html, Callback, Component, Context, Html};
use yew_mdc_widgets::dom::existing::JsObjectAccess;
use yew_mdc_widgets::dom::{self, JsValue};
use yew_mdc_widgets::wasm_bindgen::prelude::{wasm_bindgen, JsError};
use yew_mdc_widgets::wasm_bindgen::{self};
use yew_mdc_widgets::{
    auto_init, console, Button, Chip, ChipSet, CustomEvent, Dialog, Drawer, Element, IconButton, List, ListItem,
    MdcWidget, Switch, TopAppBar,
};

use self::i18n::label::*;

mod i18n;

type Lapp = CommonLapp<String>;
type LappResponse = CommonLappResponse<'static, Cow<'static, LappSettings>>;

struct Root {
    lapps: Vec<LappSettings>,
}

#[derive(Debug)]
struct PermissionUpdate {
    lapp_name: String,
    permission: Permission,
    allow: bool,
}

impl PermissionUpdate {
    fn try_from_chip_selection_detail(detail: JsValue) -> anyhow::Result<Self> {
        let chip_id = detail
            .get("chipId")
            .as_string()
            .ok_or_else(|| anyhow!("Detail chipId param does not exist"))?;
        let id_data: Vec<_> = chip_id.split("--").collect();

        #[allow(clippy::get_first)]
        if let (Some(lapp_name), Some(permission)) = (id_data.get(0), id_data.get(1)) {
            Ok(Self {
                lapp_name: lapp_name.to_string(),
                permission: Permission::try_from(*permission)?,
                allow: detail
                    .get("selected")
                    .as_bool()
                    .ok_or_else(|| anyhow!("Detail selected param does not exist"))?,
            })
        } else {
            Err(anyhow!("Wrong data of chipId: {:?}", id_data))
        }
    }
}

#[derive(Debug)]
enum Msg {
    Fetch(LappResponse),
    SwitchLapp(String),
    UpdatePermission(PermissionUpdate),
    AddLar,
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
        Self::send_get(ctx, Lapp::main_uri("lapps"));
        Self { lapps: vec![] }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Fetch(response) => match response {
                LappResponse::Lapps { lapps, .. } => {
                    self.lapps = lapps
                        .into_iter()
                        .map(|lapp_settings| lapp_settings.into_owned())
                        .collect();
                    true
                },
                LappResponse::Updated { updated } => {
                    if let Some(lapp_settings) = self
                        .lapps
                        .iter_mut()
                        .find(|lapp_settings| lapp_settings.name() == updated.lapp_name)
                    {
                        let mut should_render = false;

                        if let Some(enabled) = updated.enabled {
                            if lapp_settings.enabled() != enabled {
                                lapp_settings.set_enabled(enabled);
                                should_render = true;
                            }
                        }

                        if let Some(permission) = updated.allow_permission {
                            should_render = lapp_settings.permissions.allow(permission);
                        }

                        if let Some(permission) = updated.deny_permission {
                            should_render = lapp_settings.permissions.deny(permission);
                        }

                        should_render
                    } else {
                        console::error!(&format!("Unknown lapp name: {}", updated.lapp_name));
                        false
                    }
                },
            },
            Msg::SwitchLapp(name) => {
                if let Some(lapp) = self.lapps.iter_mut().find(|lapp| lapp.name() == name) {
                    lapp.switch_enabled();

                    let uri = Lapp::main_uri("lapp/update");
                    if let Ok(body) = serde_json::to_string(
                        &UpdateQuery::new(lapp.name().to_string())
                            .enabled(lapp.enabled())
                            .into_request(),
                    )
                    .context("Serialize query error")
                    .msg_error_map(ctx.link())
                    {
                        Self::send_post_json(ctx, uri, body);
                    }
                    false
                } else {
                    console::error!(&format!("Unknown lapp name: {name}"));
                    false
                }
            },
            Msg::UpdatePermission(PermissionUpdate {
                lapp_name,
                permission,
                allow,
            }) => {
                let uri = Lapp::main_uri("lapp/update");
                if let Ok(body) = serde_json::to_string(
                    &UpdateQuery::new(lapp_name)
                        .update_permission(permission, allow)
                        .into_request(),
                )
                .context("Serialize query error")
                .msg_error_map(ctx.link())
                {
                    Self::send_post_json(ctx, uri, body);
                }
                false
            },
            Msg::AddLar => false,
            Msg::Error(err) => {
                console::error!(&format!("{err}"));
                true
            },
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let i18n = i18n::load();

        let drawer = Drawer::new()
            .id("app-drawer")
            .title(html! { <h3 tabindex = 0>{ i18n.text(SETTINGS) }</h3> })
            .content(
                List::ul()
                    .divider()
                    .item(
                        ListItem::new()
                            .icon("upload")
                            .text(i18n.text(ADD_LAPP))
                            .attr("tabindex", "0")
                            .on_click(|_| {
                                dom::existing::get_element_by_id::<Element>("app-drawer")
                                    .get("MDCDrawer")
                                    .set("open", false);
                                Dialog::open_existing("add-lapp-dialog");
                            }),
                    )
                    .markup_only(),
            )
            .modal();

        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("laplace")
            .navigation_item(IconButton::new().icon("menu"))
            .enable_shadow_when_scroll_window()
            .on_navigation(|_| {
                let drawer = dom::existing::get_element_by_id::<Element>("app-drawer").get("MDCDrawer");
                let opened = drawer.get("open").as_bool().unwrap_or(false);
                drawer.set("open", !opened);
            });

        let add_lapp_dialog = Dialog::new()
            .id("add-lapp-dialog")
            .title(html! { <h2 tabindex = 0> { i18n.text(ADD_LAPP) } </h2> })
            .content(List::ul().item(html! {
                <div>
                    <input id = "lar-selector" name = "lar" type = "file" accept = ".lar, .zip" />
                </div>
            }))
            .action(
                Button::new()
                    .label("Cancel")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(|_| Dialog::close_existing("add-lapp-dialog")),
            )
            .action(Button::new().label("Add").class(Dialog::BUTTON_CLASS).on_click({
                let send_lar_callback = callback(ctx);

                ctx.link().callback(move |_| {
                    let files = dom::existing::get_element_by_id::<HtmlInputElement>("lar-selector").files();
                    if let Some(file) = files.and_then(|files| files.get(0)) {
                        match FormData::new() {
                            Ok(form_data) => {
                                if let Err(err) = form_data.append_with_blob("lar", &file) {
                                    return Msg::Error(anyhow!("Append file to form data error: {:?}", err));
                                }

                                let callback = send_lar_callback.clone();
                                JsonFetcher::send_post(Lapp::main_uri("lapp/add"), form_data, move |response_result| {
                                    callback.emit(response_result)
                                });
                                Dialog::close_existing("add-lapp-dialog");
                            },
                            Err(err) => return Msg::Error(anyhow!("Creation form data error: {:?}", err)),
                        }
                    }
                    Msg::AddLar
                })
            }));

        html! {
            <>
                { drawer }
                <div class="mdc-drawer-scrim"></div>

                <div class = { classes!("app-content", Drawer::APP_CONTENT_CLASS) }>
                    { top_app_bar }
                    { add_lapp_dialog }

                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ i18n.text(APPLICATIONS) }</h1>
                            <div class = "lapps-table">
                                { self.lapps.iter().map(|lapp| self.view_lapp(ctx, lapp)).collect::<Html>() }
                            </div>
                        </div>
                    </div>
                </div>
            </>
        }
    }

    fn rendered(&mut self, _ctx: &Context<Self>, _first_render: bool) {
        auto_init();
    }
}

impl Root {
    pub fn send_get(ctx: &Context<Self>, uri: impl AsRef<str>) {
        let callback = callback(ctx);
        JsonFetcher::send_get(uri, move |response_result| callback.emit(response_result));
    }

    pub fn send_post_json(ctx: &Context<Self>, uri: impl AsRef<str>, body: impl Into<JsValue>) {
        let callback = callback(ctx);
        JsonFetcher::send_post_json(uri, body, move |response_result| callback.emit(response_result));
    }

    fn view_lapp(&self, ctx: &Context<Self>, lapp_settings: &LappSettings) -> Html {
        let lapp_name = lapp_settings.name().to_string();

        let enable_switch = Switch::new()
            .on_click(ctx.link().callback(move |_| Msg::SwitchLapp(lapp_name.clone())))
            .turn(lapp_settings.enabled());

        let permissions = ChipSet::new()
            .id(format!("{}--permissions", lapp_settings.name()))
            .filter()
            .chips(lapp_settings.permissions.required().map(|permission| {
                Chip::simple()
                    .id(format!("{}--{}", lapp_settings.name(), permission.as_str()))
                    .checkmark()
                    .text(permission.as_str())
                    .select(lapp_settings.permissions.is_allowed(permission))
            }))
            .on_selection(ctx.link().callback(|event: CustomEvent| {
                PermissionUpdate::try_from_chip_selection_detail(event.detail())
                    .map(Msg::UpdatePermission)
                    .unwrap_or_else(Msg::Error)
            }));

        let lapp_ref = if let Some(access_token) = lapp_settings.application.access_token.as_deref() {
            format!("{}?access_token={access_token}", lapp_settings.name())
        } else {
            lapp_settings.name().to_string()
        };

        html! {
            <>
                <div class = "lapps-table-row">
                    <div class = "lapps-table-col">
                        <big><a href = { lapp_ref }>{ lapp_settings.title() }</a></big>
                    </div>
                    <div class = "lapps-table-col">
                        { enable_switch }
                    </div>
                </div>
                <div class = "lapps-table-row">
                    <div class = "lapps-table-col">
                        { permissions }
                    </div>
                </div>
            </>
        }
    }
}

fn callback(ctx: &Context<Root>) -> Callback<Result<(Response, Result<LappResponse>)>> {
    ctx.link()
        .callback(|response_result: Result<(Response, Result<LappResponse>)>| {
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

#[wasm_bindgen]
pub fn add_translations(translations: JsValue) -> std::result::Result<(), JsError> {
    let translations: Vec<(String, HashMap<String, String>)> = serde_wasm_bindgen::from_value(translations)?;
    i18n::add_translations(translations);
    Ok(())
}

#[wasm_bindgen]
pub fn switch_lang(lang: &str) -> bool {
    i18n::switch_lang(lang)
}

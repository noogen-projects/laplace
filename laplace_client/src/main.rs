use std::{borrow::Cow, convert::TryFrom};

use anyhow::{anyhow, Context as _, Error};
use laplace_common::{
    api::{Response as CommonLappResponse, UpdateQuery},
    lapp::{Lapp as CommonLapp, Permission},
};
use laplace_yew::error::MsgError;
use wasm_web_helpers::{
    error::Result,
    fetch::{JsonFetcher, Response},
};
use yew::{self, classes, html, Callback, Component, Context, Html};
use yew_mdc_widgets::{
    auto_init, console,
    dom::{self, existing::JsObjectAccess, JsValue},
    Chip, ChipSet, CustomEvent, Drawer, Element, IconButton, MdcWidget, Switch, TopAppBar,
};

type Lapp = CommonLapp<String>;
type LappResponse = CommonLappResponse<'static, String, Cow<'static, CommonLapp<String>>>;

struct Root {
    lapps: Vec<Lapp>,
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
                    self.lapps = lapps.into_iter().map(|lapp| lapp.into_owned()).collect();
                    true
                },
                LappResponse::Updated { updated } => {
                    if let Some(lapp) = self.lapps.iter_mut().find(|lapp| lapp.name() == updated.lapp_name) {
                        let mut should_render = false;

                        if let Some(enabled) = updated.enabled {
                            if lapp.enabled() != enabled {
                                lapp.set_enabled(enabled);
                                should_render = true;
                            }
                        }

                        if let Some(permission) = updated.allow_permission {
                            should_render = lapp.allow_permission(permission);
                        }

                        if let Some(permission) = updated.deny_permission {
                            should_render = lapp.deny_permission(permission);
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

                    let uri = Lapp::main_uri("lapp");
                    if let Ok(body) = serde_json::to_string(
                        &UpdateQuery::new(lapp.name().to_string())
                            .enabled(lapp.enabled())
                            .into_request(),
                    )
                    .context("Serialize query error")
                    .msg_error_map(ctx.link())
                    {
                        Self::send_post(ctx, uri, body);
                    }
                    false
                } else {
                    console::error!(&format!("Unknown lapp name: {}", name));
                    false
                }
            },
            Msg::UpdatePermission(PermissionUpdate {
                lapp_name,
                permission,
                allow,
            }) => {
                let uri = Lapp::main_uri("lapp");
                if let Ok(body) = serde_json::to_string(
                    &UpdateQuery::new(lapp_name)
                        .update_permission(permission, allow)
                        .into_request(),
                )
                .context("Serialize query error")
                .msg_error_map(ctx.link())
                {
                    Self::send_post(ctx, uri, body);
                }
                false
            },
            Msg::Error(err) => {
                console::error!(&format!("{}", err));
                true
            },
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let drawer = Drawer::new()
            .id("app-drawer")
            .title(html! { <h3 tabindex = 0>{ "Settings" }</h3> })
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

        html! {
            <>
                { drawer }
                <div class="mdc-drawer-scrim"></div>

                <div class = { classes!("app-content", Drawer::APP_CONTENT_CLASS) }>
                    { top_app_bar }

                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Applications" }</h1>
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

    pub fn send_post(ctx: &Context<Self>, uri: impl AsRef<str>, body: impl Into<String>) {
        let callback = callback(ctx);
        JsonFetcher::send_post(uri, body, move |response_result| callback.emit(response_result));
    }

    fn view_lapp(&self, ctx: &Context<Self>, lapp: &Lapp) -> Html {
        let lapp_name = lapp.name().to_string();

        let enable_switch = Switch::new()
            .on_click(ctx.link().callback(move |_| Msg::SwitchLapp(lapp_name.clone())))
            .turn(lapp.enabled());

        let permissions = ChipSet::new()
            .id(format!("{}--permissions", lapp.name()))
            .filter()
            .chips(lapp.required_permissions().map(|permission| {
                Chip::simple()
                    .id(format!("{}--{}", lapp.name(), permission.as_str()))
                    .checkmark()
                    .text(permission.as_str())
                    .select(lapp.is_allowed_permission(permission))
            }))
            .on_selection(ctx.link().callback(|event: CustomEvent| {
                PermissionUpdate::try_from_chip_selection_detail(event.detail())
                    .map(Msg::UpdatePermission)
                    .unwrap_or_else(Msg::Error)
            }));

        let lapp_ref = if let Some(access_token) = lapp.settings().application.access_token.as_deref() {
            format!("{}?access_token={}", lapp.name(), access_token)
        } else {
            lapp.name().to_string()
        };

        html! {
            <>
                <div class = "lapps-table-row">
                    <div class = "lapps-table-col">
                        <big><a href = { lapp_ref }>{ lapp.title() }</a></big>
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

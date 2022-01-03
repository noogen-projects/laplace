use std::{borrow::Cow, convert::TryFrom};

use anyhow::{anyhow, Context, Error, Result};
use laplace_common::{
    api::{Response as CommonLappResponse, UpdateQuery},
    lapp::{Lapp as CommonLapp, Permission},
};
use laplace_yew::{error::MsgError, fetch::JsonFetcher};
use yew::{html, initialize, run_loop, services::ConsoleService, utils, App, Component, ComponentLink, Html};
use yew_mdc_widgets::{
    auto_init,
    utils::dom::{self, JsObjectAccess, JsValue},
    Chip, ChipSet, CustomEvent, Drawer, Element, IconButton, MdcWidget, Switch, TopAppBar,
};

type Lapp = CommonLapp<String>;
type LappResponse = CommonLappResponse<'static, String, Cow<'static, CommonLapp<String>>>;

struct Root {
    lapps: Vec<Lapp>,
    link: ComponentLink<Self>,
    fetcher: JsonFetcher,
}

#[derive(Debug)]
struct PermissionUpdate {
    lapp_name: String,
    permission: Permission,
    allow: bool,
}

impl PermissionUpdate {
    fn try_from_chip_selection_detail(detail: JsValue) -> Result<Self> {
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

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut root = Self {
            lapps: vec![],
            link,
            fetcher: JsonFetcher::new(),
        };

        root.send_get(Lapp::main_uri("lapps"))
            .context("Get lapps list error")
            .msg_error(&root.link);
        root
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Fetch(response) => match response {
                LappResponse::Lapps { lapps, .. } => {
                    self.lapps = lapps.into_iter().map(|lapp| lapp.into_owned()).collect();
                    true
                },
                LappResponse::Updated(updated) => {
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
                        ConsoleService::error(&format!("Unknown lapp name: {}", updated.lapp_name));
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
                    .msg_error_map(&self.link)
                    {
                        self.send_post(uri, body)
                            .context("Switch lapp error")
                            .msg_error(&self.link);
                    }
                    false
                } else {
                    ConsoleService::error(&format!("Unknown lapp name: {}", name));
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
                .msg_error_map(&self.link)
                {
                    self.send_post(uri, body)
                        .context("Select permission error")
                        .msg_error(&self.link);
                }
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
                let drawer = dom::get_exist_element_by_id::<Element>("app-drawer").get("MDCDrawer");
                let opened = drawer.get("open").as_bool().unwrap_or(false);
                drawer.set("open", !opened);
            });

        html! {
            <>
                { drawer }
                <div class="mdc-drawer-scrim"></div>

                <div class = vec!["app-content", Drawer::APP_CONTENT_CLASS]>
                    { top_app_bar }

                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Applications" }</h1>
                            <div class = "lapps-table">
                                { self.lapps.iter().map(|lapp| self.view_lapp(lapp)).collect::<Html>() }
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
    pub fn send_get(&mut self, uri: impl AsRef<str>) -> Result<()> {
        let callback = JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error);
        self.fetcher.send_get(uri, callback)
    }

    pub fn send_post(&mut self, uri: impl AsRef<str>, body: impl Into<String>) -> Result<()> {
        let callback = JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error);
        self.fetcher.send_post(uri, body, callback)
    }

    fn view_lapp(&self, lapp: &Lapp) -> Html {
        let lapp_name = lapp.name().to_string();

        let enable_switch = Switch::new()
            .on_click(self.link.callback(move |_| Msg::SwitchLapp(lapp_name.clone())))
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
            .on_selection(self.link.callback(|event: CustomEvent| {
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
                        <big><a href = lapp_ref>{ lapp.title() }</a></big>
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

fn main() {
    initialize();
    if let Ok(Some(root)) = utils::document().query_selector("#root") {
        App::<Root>::new().mount_with_props(root, ());
        run_loop();
    } else {
        ConsoleService::error("Can't get root node for rendering");
    }
}

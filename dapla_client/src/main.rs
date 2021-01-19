use yew::{html, initialize, run_loop, services::ConsoleService, utils, App, Component, ComponentLink, Html};
use yew_mdc_widgets::{auto_init, Drawer, IconButton, MdcWidget, TopAppBar};

struct Root {
    link: ComponentLink<Self>,
}

impl Component for Root {
    type Message = ();
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self { link }
    }

    fn update(&mut self, _msg: Self::Message) -> bool {
        false
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let drawer_id = "app-drawer";
        let drawer = Drawer::new()
            .id(drawer_id)
            .title(html! { <h3 tabindex = 0>{ "Settings" }</h3> })
            .modal();

        let top_app_bar = TopAppBar::new()
            .id("top-app-bar")
            .title("dapla")
            .navigation_item(IconButton::new().icon("menu"))
            .enable_shadow_when_scroll_window()
            .add_navigation_event(format!(
                r"{{
                    const drawer = document.getElementById('{}').MDCDrawer;
                    drawer.open = !drawer.open;
                }}",
                drawer_id
            ));

        html! {
            <>
                { drawer }
                <div class="mdc-drawer-scrim"></div>

                <div class = vec!["app-content", Drawer::APP_CONTENT_CLASS]>
                    { top_app_bar }
                    <script>{ format!(r"
                        const listEl = document.querySelector('.mdc-drawer .mdc-list');
                        listEl.addEventListener('click', (event) => {{
                            document.getElementById('{}').MDCDrawer.open = false;
                        }});
                    ", drawer_id) }</script>

                    <div class = "mdc-top-app-bar--fixed-adjust">
                        <div class = "content-container">
                            <h1 class = "title mdc-typography--headline5">{ "Applications" }</h1>
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

fn main() {
    initialize();
    if let Ok(Some(root)) = utils::document().query_selector("#root") {
        App::<Root>::new().mount_with_props(root, ());
        run_loop();
    } else {
        ConsoleService::error("Can't get root node for rendering");
    }
}

use std::collections::HashMap;
use std::marker::PhantomData;

use yew::html::Scope;
use yew::{html, Component, Context, Html, Properties};
use yew_mdc_widgets::{IconButton, MdcWidget, Snackbar};

pub const DEFAULT_ERRORS_ID: &str = "errors-snackbar";

#[derive(Debug)]
pub struct Errors<ParentT> {
    id: String,
    errors: HashMap<String, usize>,
    timeout_ms: i32,
    _phantom: PhantomData<ParentT>,
}

pub enum ErrorsMsg {
    Open,
    Close,
    Add(String),
    Spawn(String),
}

#[derive(Properties, PartialEq)]
pub struct ErrorsProps {
    #[prop_or(String::from(DEFAULT_ERRORS_ID))]
    pub id: String,

    #[prop_or(-1)]
    pub timeout_ms: i32,

    #[prop_or_default]
    pub errors: HashMap<String, usize>,
}

impl<ParentT> Component for Errors<ParentT>
where
    ParentT: Component,
    ParentT::Message: From<Scope<Self>>,
{
    type Message = ErrorsMsg;
    type Properties = ErrorsProps;

    fn create(ctx: &Context<Self>) -> Self {
        if let Some(parent_link) = ctx.link().get_parent() {
            parent_link.downcast::<ParentT>().send_message(ctx.link().clone());
        }

        Self {
            id: ctx.props().id.clone(),
            timeout_ms: ctx.props().timeout_ms,
            errors: ctx.props().errors.clone(),
            _phantom: PhantomData,
        }
    }

    fn update(&mut self, _ctx: &yew::Context<Self>, msg: Self::Message) -> bool {
        match msg {
            ErrorsMsg::Open => self.open(),
            ErrorsMsg::Close => self.close(),
            ErrorsMsg::Add(error) => self.add(error),
            ErrorsMsg::Spawn(error) => {
                self.add(error);
                self.open();
            },
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let messages = self
            .errors
            .iter()
            .map(|(error, count)| {
                let message = format!("({count}) {error}");
                html! { <div>{ message }</div> }
            })
            .collect::<Html>();

        Snackbar::new()
            .id(&self.id)
            .label(messages)
            .dismiss(
                IconButton::new()
                    .icon("close")
                    .on_click(ctx.link().callback(move |_| ErrorsMsg::Close)),
            )
            .into()
    }
}

impl<ParentT> Errors<ParentT> {
    fn open(&self) {
        Snackbar::set_timeout_ms(&self.id, self.timeout_ms);
        Snackbar::open_existing(&self.id);
    }

    fn close(&mut self) {
        self.errors.clear();
        Snackbar::close_existing(&self.id);
    }

    fn add(&mut self, error: impl Into<String>) {
        *self.errors.entry(error.into()).or_default() += 1;
    }
}

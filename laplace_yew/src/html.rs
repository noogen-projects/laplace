use wasm_dom::UnwrapThrowExt;
use web_sys::Node;
use yew::{virtual_dom::VNode, Component, Context, Html, Properties};

#[derive(Debug, Clone, Eq, PartialEq, Properties)]
pub struct RawHtmlProps {
    pub inner_html: String,
}

pub struct RawHtml {
    props: RawHtmlProps,
}

impl Component for RawHtml {
    type Message = ();
    type Properties = RawHtmlProps;

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            props: ctx.props().clone(),
        }
    }

    fn changed(&mut self, ctx: &Context<Self>) -> bool {
        if self.props != *ctx.props() {
            self.props = ctx.props().clone();
            true
        } else {
            false
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {
        let div = wasm_dom::existing::document()
            .create_element("div")
            .expect_throw("Div should be created");
        div.set_inner_html(self.props.inner_html.as_str());

        VNode::VRef(Node::from(div))
    }
}

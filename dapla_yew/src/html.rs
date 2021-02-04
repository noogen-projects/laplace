use yew::{
    utils::document, virtual_dom::VNode, web_sys::Node, Component, ComponentLink, Html, Properties, ShouldRender,
};

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

    fn create(props: Self::Properties, _: ComponentLink<Self>) -> Self {
        Self { props }
    }

    fn update(&mut self, _: Self::Message) -> ShouldRender {
        true
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.props != props {
            self.props = props;
            true
        } else {
            false
        }
    }

    fn view(&self) -> Html {
        let div = document().create_element("div").expect("Div should be created");
        div.set_inner_html(self.props.inner_html.as_str());

        VNode::VRef(Node::from(div))
    }
}

use gloo_timers::callback::Timeout;
use web_sys::HtmlInputElement;
use yew::html::Scope;
use yew::{html, Component, Context, Html, MouseEvent, Properties};
use yew_mdc_widgets::dom::{self, JsCast};
use yew_mdc_widgets::{console, Button, Dialog, Element, IconButton, List, ListItem, MdcWidget, TextField};

use super::{Msg as RootMsg, Root};

pub(super) struct Addresses {
    root_link: Scope<Root>,
    list: Vec<String>,
}

pub(super) enum Msg {
    Add(String),
    Remove(usize),
    FinishRemove(String),
}

#[derive(Properties, Clone)]
pub(super) struct Props {
    pub(super) root: Scope<Root>,
    pub(super) list: Vec<String>,
}

impl PartialEq for Props {
    fn eq(&self, other: &Self) -> bool {
        self.list.eq(&other.list)
    }
}

impl Component for Addresses {
    type Message = Msg;
    type Properties = Props;

    fn create(ctx: &Context<Self>) -> Self {
        ctx.props()
            .root
            .send_message(RootMsg::LinkAddresses(ctx.link().clone()));
        Self {
            root_link: ctx.props().root.clone(),
            list: ctx.props().list.clone(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Add(address) => {
                if !address.is_empty() && !self.list.contains(&address) {
                    self.root_link.send_message(RootMsg::AddAddress(address.clone()));
                    self.list.push(address);
                    true
                } else {
                    false
                }
            },
            Msg::Remove(index) => {
                let address = self.list.remove(index);
                Timeout::new(200, {
                    let callback = ctx.link().callback(move |_| Msg::FinishRemove(address.clone()));
                    move || callback.emit(())
                })
                .forget();
                false
            },
            Msg::FinishRemove(address) => {
                console::log!(&format!("Remove {address}"));
                dom::existing::get_element_by_id::<Element>(&address).remove();
                false
            },
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let address_items = self.list.iter().enumerate().map(|(index, address)| {
            ListItem::new()
                .id(address)
                .text(html! { <strong>{ address }</strong> })
                .tile(IconButton::new().icon("close").on_click(ctx.link().callback({
                    let address = address.clone();
                    move |_| {
                        let item = dom::existing::get_element_by_id::<Element>(&address);
                        item.set_class_name(&format!("{} exited", item.class_name()));
                        Msg::Remove(index)
                    }
                })))
                .on_click(|event: MouseEvent| {
                    if let Ok(range) = dom::existing::document().create_range() {
                        if let Some(element) = event
                            .target()
                            .and_then(|target| JsCast::dyn_into::<Element>(target).ok())
                        {
                            let node = element.children().get_with_index(1).unwrap_or(element);

                            if (node.tag_name() == "STRONG" || node.class_name() == ListItem::TEXT_ITEM_CLASS)
                                && range.select_node_contents(&node).is_ok()
                            {
                                if let Ok(Some(selection)) = dom::existing::window().get_selection() {
                                    selection.remove_all_ranges().ok();
                                    selection.add_range(&range).ok();
                                }
                            }
                        }
                    }
                })
        });

        Dialog::new()
            .id("addresses-dialog")
            .title(html! { <h2 tabindex = 0> { "Addresses" } </h2> })
            .content(List::ul().id("addresses-list").items(address_items))
            .action(
                TextField::outlined()
                    .id("new-address")
                    .class("address-textfield")
                    .label("New address"),
            )
            .action(
                Button::new()
                    .label("Add")
                    .class(Dialog::BUTTON_CLASS)
                    .on_click(ctx.link().callback(move |_| {
                        let address = dom::existing::select_element::<HtmlInputElement>("#new-address > input").value();
                        Msg::Add(address)
                    })),
            )
            .into()
    }
}

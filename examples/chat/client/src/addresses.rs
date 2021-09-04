use std::time::Duration;

use web_sys::HtmlInputElement;
use yew::{
    html,
    services::{
        console::ConsoleService,
        timeout::{TimeoutService, TimeoutTask},
        Task,
    },
    Component, ComponentLink, Html, MouseEvent, Properties, ShouldRender,
};
use yew_mdc_widgets::{
    utils::dom::{self, JsCast},
    Button, Dialog, Element, IconButton, List, ListItem, MdcWidget, TextField,
};

use super::{Msg as RootMsg, Root};

pub(super) struct Addresses {
    root_link: ComponentLink<Root>,
    this_link: ComponentLink<Self>,
    timeout_tasks: Vec<TimeoutTask>,
    list: Vec<String>,
}

pub(super) enum Msg {
    Add(String),
    Remove(usize),
    FinishRemove(String),
}

#[derive(Properties, Clone)]
pub(super) struct Props {
    pub(super) root: ComponentLink<Root>,
    pub(super) list: Vec<String>,
}

impl Component for Addresses {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        props.root.send_message(RootMsg::LinkAddresses(link.clone()));
        Self {
            root_link: props.root,
            this_link: link,
            timeout_tasks: Vec::new(),
            list: props.list,
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
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
                let handle = TimeoutService::spawn(
                    Duration::from_millis(200),
                    self.this_link.callback(move |_| Msg::FinishRemove(address.clone())),
                );
                self.timeout_tasks.push(handle);
                false
            },
            Msg::FinishRemove(address) => {
                ConsoleService::log(&format!("Remove {}", address));
                dom::get_exist_element_by_id::<Element>(&address).remove();
                let to_remove_indexes: Vec<_> = self
                    .timeout_tasks
                    .iter()
                    .enumerate()
                    .filter_map(|(index, task)| if !task.is_active() { Some(index) } else { None })
                    .collect();
                for task_index in to_remove_indexes {
                    let _ = self.timeout_tasks.remove(task_index);
                }
                false
            },
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn view(&self) -> Html {
        let address_items = self.list.iter().enumerate().map(|(index, address)| {
            ListItem::new()
                .id(address)
                .text(html! { <strong>{ address }</strong> })
                .tile(IconButton::new().icon("close").on_click(self.this_link.callback({
                    let address = address.clone();
                    move |_| {
                        let item = dom::get_exist_element_by_id::<Element>(&address);
                        item.set_class_name(&format!("{} exited", item.class_name()));
                        Msg::Remove(index)
                    }
                })))
                .on_click(|event: MouseEvent| {
                    if let Ok(range) = dom::document().create_range() {
                        if let Some(element) = event
                            .target()
                            .and_then(|target| JsCast::dyn_into::<Element>(target).ok())
                        {
                            let node = element.children().get_with_index(1).unwrap_or(element);

                            if (node.tag_name() == "STRONG" || node.class_name() == ListItem::TEXT_ITEM_CLASS)
                                && range.select_node_contents(&node).is_ok()
                            {
                                if let Ok(Some(selection)) = dom::window().get_selection() {
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
                    .on_click(self.this_link.callback(move |_| {
                        let address = dom::select_exist_element::<HtmlInputElement>("#new-address > input").value();
                        Msg::Add(address)
                    })),
            )
            .into()
    }
}

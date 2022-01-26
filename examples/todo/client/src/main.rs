#![recursion_limit = "512"]

use anyhow::{anyhow, Error};
use gloo_console as console;
use laplace_yew::MsgError;
use strum::{Display, EnumIter, IntoEnumIterator};
use todo_common::{Response, Task};
use wasm_web_helpers::{
    error::Result,
    fetch::{JsonFetcher, Response as WebResponse},
};
use web_sys::HtmlInputElement;
use yew::{classes, html, Callback, Component, Context, Html, InputEvent, KeyboardEvent, NodeRef};

#[derive(EnumIter, Display, Clone, Copy, PartialEq)]
enum Filter {
    All,
    Active,
    Completed,
}

impl Filter {
    fn fit(&self, task: &Task) -> bool {
        match self {
            Filter::All => true,
            Filter::Active => !task.completed,
            Filter::Completed => task.completed,
        }
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::All
    }
}

struct Edit {
    value: String,
    task_idx: usize,
}

#[derive(Default)]
struct TodoState {
    list: Vec<Task>,
    filter: Filter,
    value: String,
    edit: Option<Edit>,
}

impl TodoState {
    fn filtered_task_idx(&mut self, idx: usize) -> usize {
        let filter = self.filter;
        self.list
            .iter_mut()
            .enumerate()
            .filter(|(_, task)| filter.fit(task))
            .nth(idx)
            .map(|(idx, _)| idx)
            .unwrap()
    }

    fn total(&self) -> usize {
        self.list.len()
    }

    fn total_completed(&self) -> usize {
        self.list.iter().filter(|task| Filter::Completed.fit(task)).count()
    }

    fn is_all_completed(&self) -> bool {
        let mut filtered_iter = self.list.iter().filter(|task| self.filter.fit(task)).peekable();

        if filtered_iter.peek().is_none() {
            return false;
        }

        filtered_iter.all(|task| task.completed)
    }

    fn toggle(&mut self, idx: usize) -> usize {
        let idx = self.filtered_task_idx(idx);
        let task = &mut self.list[idx];
        task.completed = !task.completed;
        idx
    }

    fn remove(&mut self, idx: usize) -> usize {
        let idx = self.filtered_task_idx(idx);
        self.list.remove(idx);
        idx
    }
}

enum Msg {
    Add,
    Edit,
    TypeNew,
    TypeEdit(usize),
    Save(usize),
    Remove(usize),
    SetFilter(Filter),
    ToggleAll,
    ToggleEdit(usize),
    Toggle(usize),
    ClearCompleted,
    Focus,
    Fetch(Response),
    Error(Error),
    Nope,
}

impl From<Error> for Msg {
    fn from(err: Error) -> Self {
        Self::Error(err)
    }
}

struct Root {
    state: TodoState,
    focus_ref: NodeRef,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        JsonFetcher::send_get("/todo/list", {
            let callback = callback(ctx);
            move |response_result| callback.emit(response_result)
        });

        Self {
            state: Default::default(),
            focus_ref: Default::default(),
        }
    }

    fn update(&mut self, ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::Add => {
                let description = self.state.value.trim();
                if !description.is_empty() {
                    JsonFetcher::send_post(
                        "/todo/add",
                        format!(r#"{{"description":"{}","completed":false}}"#, description),
                        {
                            let callback = callback(ctx);
                            move |response_result| callback.emit(response_result)
                        },
                    );
                }
                self.state.value.clear();
                false
            },
            Msg::Edit => {
                if let Some(edit) = self.state.edit.take() {
                    let idx = self.state.filtered_task_idx(edit.task_idx);
                    let description = edit.value.trim();

                    let msg = if !description.is_empty() {
                        self.state.list[idx].description = description.to_string();
                        Msg::Save(idx)
                    } else {
                        Msg::Remove(idx)
                    };
                    ctx.link().send_message(msg);
                }
                false
            },
            Msg::TypeNew => {
                let value = wasm_dom::existing::get_element_by_id::<HtmlInputElement>("new-task-input").value();
                self.state.value = value;
                false
            },
            Msg::TypeEdit(idx) => {
                if let Some(edit) = &mut self.state.edit {
                    let value =
                        wasm_dom::existing::get_element_by_id::<HtmlInputElement>(&format!("edit-task-{}", idx))
                            .value();
                    edit.value = value;
                }
                false
            },
            Msg::Save(idx) => {
                let task = &self.state.list[idx];
                JsonFetcher::send_post(
                    format!("/todo/update/{}", idx + 1),
                    format!(
                        r#"{{"description":"{}","completed":{}}}"#,
                        task.description, task.completed
                    ),
                    {
                        let callback = callback(ctx);
                        move |response_result| callback.emit(response_result)
                    },
                );
                false
            },
            Msg::Remove(idx) => {
                let idx = self.state.remove(idx);
                JsonFetcher::send_post(format!("/todo/delete/{}", idx + 1), "", {
                    let callback = callback(ctx);
                    move |response_result| callback.emit(response_result)
                });
                false
            },
            Msg::SetFilter(filter) => {
                self.state.filter = filter;
                true
            },
            Msg::ToggleEdit(idx) => {
                self.state.edit = Some(Edit {
                    value: self.state.list[idx].description.clone(),
                    task_idx: idx,
                });
                true
            },
            Msg::ToggleAll => {
                let status = !self.state.is_all_completed();
                for (idx, task) in self.state.list.iter_mut().enumerate() {
                    if self.state.filter.fit(task) && task.completed != status {
                        task.completed = status;
                        ctx.link().send_message(Msg::Save(idx));
                    }
                }
                false
            },
            Msg::Toggle(idx) => {
                let idx = self.state.toggle(idx);
                ctx.link().send_message(Msg::Save(idx));
                false
            },
            Msg::ClearCompleted => {
                JsonFetcher::send_post("/todo/clear_completed", "", {
                    let callback = callback(ctx);
                    move |response_result| callback.emit(response_result)
                });
                false
            },
            Msg::Focus => {
                if let Some(input) = self.focus_ref.cast::<HtmlInputElement>() {
                    input
                        .focus()
                        .map_err(|err| anyhow!("Input focus error: {:?}", err))
                        .msg_error(ctx.link());
                }
                false
            },
            Msg::Fetch(Response::List(list)) => {
                self.state.list = list;
                true
            },
            Msg::Fetch(Response::Task(task)) => {
                self.state.list.push(task);
                true
            },
            Msg::Fetch(Response::Empty) => true,
            Msg::Fetch(Response::Error(err)) => {
                ctx.link().send_message(Msg::Error(anyhow!("{}", err)));
                false
            },
            Msg::Error(err) => {
                console::error!(&format!("{}", err));
                true
            },
            Msg::Nope => false,
        }
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let hidden_class = if self.state.list.is_empty() { "hidden" } else { "" };
        html! {
            <div class = "todomvc-wrapper">
                <section class = "todoapp">
                    <header class = "header">
                        <h1>{ "todos" }</h1>
                        { self.view_input(ctx) }
                    </header>
                    <section class = { classes!("main", hidden_class) }>
                        <input
                            type = "checkbox"
                            class = "toggle-all"
                            id = "toggle-all"
                            checked = { self.state.is_all_completed() }
                            onclick = { ctx.link().callback(|_| Msg::ToggleAll) } />
                        <label for = "toggle-all" />
                        <ul class = "todo-list">
                            { for self.state.list.iter().filter(|task| self.state.filter.fit(task)).enumerate().map(|task| self.view_task(ctx, task)) }
                        </ul>
                    </section>
                    <footer class = { classes!("footer", hidden_class) }>
                        <span class = "todo-count">
                            <strong>{ self.state.total() }</strong>
                            { " item(s) left" }
                        </span>
                        <ul class = "filters">
                            { for Filter::iter().map(|filter| self.view_filter(ctx, filter)) }
                        </ul>
                        <button class = "clear-completed" onclick = { ctx.link().callback(|_| Msg::ClearCompleted) }>
                            { format!("Clear completed ({})", self.state.total_completed()) }
                        </button>
                    </footer>
                </section>
                <footer class = "info">
                    <p>{ "Double-click to edit a todo" }</p>
                    <p>{ "Part of " }<a href = "http://todomvc.com/" target="_blank">{ "TodoMVC" }</a></p>
                </footer>
            </div>
        }
    }
}

impl Root {
    fn view_filter(&self, ctx: &Context<Self>, filter: Filter) -> Html {
        html! {
            <li>
                <a class = { if self.state.filter == filter { "selected" } else { "not-selected" } }
                        href = { filter.to_string() }
                        onclick = { ctx.link().callback(move |_| Msg::SetFilter(filter)) }>
                    { filter }
                </a>
            </li>
        }
    }

    fn view_input(&self, ctx: &Context<Self>) -> Html {
        html! {
            <input id = "new-task-input" class = "new-todo" placeholder = "What needs to be done?"
                    value = { self.state.value.clone() }
                    oninput = { ctx.link().callback(|_: InputEvent| Msg::TypeNew) }
                    onkeypress = { ctx.link().callback(|event: KeyboardEvent| {
                        if event.key() == "Enter" { Msg::Add } else { Msg::Nope }
                   }) } />
        }
    }

    fn view_task(&self, ctx: &Context<Self>, (idx, task): (usize, &Task)) -> Html {
        let mut classes = vec!["todo"];
        if self
            .state
            .edit
            .as_ref()
            .map(|edit| edit.task_idx == idx)
            .unwrap_or(false)
        {
            classes.push("editing");
        }
        if task.completed {
            classes.push("completed");
        }
        html! {
            <li class = { classes }>
                <div class = "view">
                    <input type = "checkbox" class = "toggle" checked = { task.completed }
                            onclick = { ctx.link().callback(move |_| Msg::Toggle(idx)) } />
                    <label ondblclick = { ctx.link().callback(move |_| Msg::ToggleEdit(idx)) } >{ &task.description }</label>
                    <button class = "destroy" onclick = { ctx.link().callback(move |_| Msg::Remove(idx)) } />
                </div>
                { self.view_task_edit_input(ctx, idx) }
            </li>
        }
    }

    fn view_task_edit_input(&self, ctx: &Context<Self>, idx: usize) -> Html {
        if let Some(Edit { value, task_idx }) = &self.state.edit {
            if *task_idx == idx {
                return html! {
                    <input id = { format!("edit-task-{}", idx) } class = "edit" type = "text" ref = { self.focus_ref.clone() } value = { value.clone() }
                            onmouseover = { ctx.link().callback(|_| Msg::Focus) }
                            oninput = { ctx.link().callback(move |_: InputEvent| Msg::TypeEdit(idx)) }
                            onblur = { ctx.link().callback(move |_| Msg::Edit) }
                            onkeypress = { ctx.link().callback(move |event: KeyboardEvent| {
                                if event.key() == "Enter" { Msg::Edit } else { Msg::Nope }
                            }) } />
                };
            }
        }
        html! { <input type = "hidden" /> }
    }
}

fn callback(ctx: &Context<Root>) -> Callback<Result<(WebResponse, Result<Response>)>> {
    ctx.link()
        .callback(|response_result: Result<(WebResponse, Result<Response>)>| {
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
    yew::start_app::<Root>();
}

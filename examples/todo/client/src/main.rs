#![recursion_limit = "512"]

use anyhow::{anyhow, Context as _, Error};
use laplace_yew::{JsonFetcher, MsgError};
use strum::{Display, EnumIter, IntoEnumIterator};
use todo_common::{Response, Task};
use web_sys::HtmlInputElement;
use yew::{
    classes, html, services::console::ConsoleService, Component, ComponentLink, Html, InputData, KeyboardEvent, NodeRef,
};

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
    TypeNew(String),
    TypeEdit(String),
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
    link: ComponentLink<Self>,
    fetcher: JsonFetcher,
    state: TodoState,
    focus_ref: NodeRef,
}

impl Component for Root {
    type Message = Msg;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let mut fetcher = JsonFetcher::new();
        fetcher
            .send_get("/todo/list", JsonFetcher::callback(&link, Msg::Fetch, Msg::Error))
            .context("Get todo list error")
            .msg_error(&link);

        Self {
            link,
            fetcher,
            state: Default::default(),
            focus_ref: Default::default(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> bool {
        match msg {
            Msg::Add => {
                let description = self.state.value.trim();
                if !description.is_empty() {
                    self.fetcher
                        .send_post(
                            "/todo/add",
                            format!(r#"{{"description":"{}","completed":false}}"#, description),
                            JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                        )
                        .context("Add task error")
                        .msg_error(&self.link);
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
                    self.link.send_message(msg);
                }
                false
            },
            Msg::TypeNew(value) => {
                self.state.value = value;
                false
            },
            Msg::TypeEdit(value) => {
                if let Some(edit) = &mut self.state.edit {
                    edit.value = value;
                }
                false
            },
            Msg::Save(idx) => {
                let task = &self.state.list[idx];
                self.fetcher
                    .send_post(
                        format!("/todo/update/{}", idx + 1),
                        format!(
                            r#"{{"description":"{}","completed":{}}}"#,
                            task.description, task.completed
                        ),
                        JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                    )
                    .context("Save task error")
                    .msg_error(&self.link);
                false
            },
            Msg::Remove(idx) => {
                let idx = self.state.remove(idx);
                self.fetcher
                    .send_post(
                        format!("/todo/delete/{}", idx + 1),
                        "",
                        JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                    )
                    .context("Remove task error")
                    .msg_error(&self.link);
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
                        self.link.send_message(Msg::Save(idx));
                    }
                }
                false
            },
            Msg::Toggle(idx) => {
                let idx = self.state.toggle(idx);
                self.link.send_message(Msg::Save(idx));
                false
            },
            Msg::ClearCompleted => {
                self.fetcher
                    .send_post(
                        "/todo/clear_completed",
                        "",
                        JsonFetcher::callback(&self.link, Msg::Fetch, Msg::Error),
                    )
                    .context("Clear completed tasks error")
                    .msg_error(&self.link);
                false
            },
            Msg::Focus => {
                if let Some(input) = self.focus_ref.cast::<HtmlInputElement>() {
                    input
                        .focus()
                        .map_err(|err| anyhow!("Input focus error: {:?}", err))
                        .msg_error(&self.link);
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
                self.link.send_message(Msg::Error(anyhow!("{}", err)));
                false
            },
            Msg::Error(err) => {
                ConsoleService::error(&format!("{}", err));
                true
            },
            Msg::Nope => false,
        }
    }

    fn change(&mut self, _props: Self::Properties) -> bool {
        false
    }

    fn view(&self) -> Html {
        let hidden_class = if self.state.list.is_empty() { "hidden" } else { "" };
        html! {
            <div class = "todomvc-wrapper">
                <section class = "todoapp">
                    <header class = "header">
                        <h1>{ "todos" }</h1>
                        { self.view_input() }
                    </header>
                    <section class = classes!("main", hidden_class)>
                        <input
                            type = "checkbox"
                            class = "toggle-all"
                            id = "toggle-all"
                            checked = self.state.is_all_completed()
                            onclick = self.link.callback(|_| Msg::ToggleAll) />
                        <label for = "toggle-all" />
                        <ul class = "todo-list">
                            { for self.state.list.iter().filter(|task| self.state.filter.fit(task)).enumerate().map(|task| self.view_task(task)) }
                        </ul>
                    </section>
                    <footer class = classes!("footer", hidden_class)>
                        <span class = "todo-count">
                            <strong>{ self.state.total() }</strong>
                            { " item(s) left" }
                        </span>
                        <ul class = "filters">
                            { for Filter::iter().map(|filter| self.view_filter(filter)) }
                        </ul>
                        <button class = "clear-completed" onclick = self.link.callback(|_| Msg::ClearCompleted)>
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
    fn view_filter(&self, filter: Filter) -> Html {
        html! {
            <li>
                <a class = if self.state.filter == filter { "selected" } else { "not-selected" }
                        href = filter.to_string()
                        onclick = self.link.callback(move |_| Msg::SetFilter(filter))>
                    { filter }
                </a>
            </li>
        }
    }

    fn view_input(&self) -> Html {
        html! {
            <input class = "new-todo" placeholder = "What needs to be done?"
                    value = self.state.value.clone()
                    oninput = self.link.callback(|event: InputData| Msg::TypeNew(event.value))
                    onkeypress = self.link.callback(|event: KeyboardEvent| {
                        if event.key() == "Enter" { Msg::Add } else { Msg::Nope }
                   }) />
        }
    }

    fn view_task(&self, (idx, task): (usize, &Task)) -> Html {
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
            <li class = classes>
                <div class = "view">
                    <input type = "checkbox" class = "toggle" checked = task.completed
                            onclick = self.link.callback(move |_| Msg::Toggle(idx)) />
                    <label ondblclick = self.link.callback(move |_| Msg::ToggleEdit(idx))>{ &task.description }</label>
                    <button class = "destroy" onclick = self.link.callback(move |_| Msg::Remove(idx)) />
                </div>
                { self.view_task_edit_input(idx) }
            </li>
        }
    }

    fn view_task_edit_input(&self, idx: usize) -> Html {
        if let Some(Edit { value, task_idx }) = &self.state.edit {
            if *task_idx == idx {
                return html! {
                    <input class = "edit" type = "text" ref = self.focus_ref.clone() value = value.clone()
                            onmouseover = self.link.callback(|_| Msg::Focus)
                            oninput = self.link.callback(|event: InputData| Msg::TypeEdit(event.value))
                            onblur = self.link.callback(move |_| Msg::Edit)
                            onkeypress = self.link.callback(move |event: KeyboardEvent| {
                                if event.key() == "Enter" { Msg::Edit } else { Msg::Nope }
                            }) />
                };
            }
        }
        html! { <input type = "hidden" /> }
    }
}

fn main() {
    yew::start_app::<Root>();
}

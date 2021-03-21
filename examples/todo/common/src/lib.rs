use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Task {
    pub description: String,
    pub completed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Response {
    List(Vec<Task>),
    Task(Task),
    Empty,
    Error(String),
}

impl Response {
    pub fn json_error_from<E: fmt::Debug>(err: E) -> String {
        format!(r#"{{"Error":"{:?}"}}"#, err)
    }
}

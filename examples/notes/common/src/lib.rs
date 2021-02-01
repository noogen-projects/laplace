use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Note {
    pub name: String,
    pub content: NoteContent,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum NoteContent {
    Preview(String),
    FullBody(String),
}

impl NoteContent {
    pub fn content(&self) -> &str {
        match self {
            NoteContent::Preview(content) => content.as_str(),
            NoteContent::FullBody(content) => content.as_str(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Response {
    Notes(Vec<Note>),
    Note(Note),
    Error(String),
}

impl Response {
    pub fn json_error_from<E: fmt::Debug>(err: E) -> String {
        format!(r#"{{"Error":"{:?}"}}"#, err)
    }
}

use std::{fmt, io, iter};

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
    pub const PREVIEW_LIMIT: usize = 300;

    pub fn content(&self) -> Option<&str> {
        match self {
            Self::Preview(_) => None,
            Self::FullBody(content) => Some(content.as_str()),
        }
    }

    pub fn preview(&self) -> Option<&str> {
        match self {
            Self::Preview(preview) => Some(preview.as_str()),
            Self::FullBody(_) => None,
        }
    }

    pub fn make_preview(&self) -> String {
        match self {
            Self::Preview(preview) => preview.clone(),
            Self::FullBody(content) => {
                make_preview(content.lines().map(|line| Ok(line.to_string()))).expect("Lines should be always Ok")
            },
        }
    }
}

pub fn make_preview(lines: impl Iterator<Item = io::Result<String>>) -> io::Result<String> {
    let mut preview = String::new();
    let mut preview_chars = 0;

    let mut prev_line = String::new();
    'lines: for line in lines {
        let line = line?;
        if line.starts_with("---") && prev_line.is_empty() {
            break 'lines;
        }

        for ch in line.chars().chain(iter::once('\n')) {
            preview.push(ch);
            preview_chars += 1;
            if preview_chars >= NoteContent::PREVIEW_LIMIT {
                break 'lines;
            }
        }
        prev_line = line;
    }

    if preview.ends_with("\n\n") {
        preview.remove(preview.len() - 1);
    }

    Ok(preview)
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Response {
    Notes(Vec<Note>),
    Note(Note),
    Error(String),
}

impl Response {
    pub fn json_error_from<E: fmt::Debug>(err: E) -> String {
        format!(r#"{{"Error":"{err:?}"}}"#)
    }
}

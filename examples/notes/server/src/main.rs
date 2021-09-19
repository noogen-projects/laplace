#![no_main]

use std::{
    fs::{self, DirEntry, File},
    io::{self, BufRead, BufReader},
    path::Path,
};

use dapla_wasm::process::{
    self,
    http::{self, Method, Uri},
};
use notes_common::{make_preview, Note, NoteContent, Response};
use thiserror::Error;

#[process::http]
fn http(request: http::Request) -> http::Response {
    let (request, body) = request.into_parts();
    let response = match request.method {
        Method::GET => NotesRequest::parse(request.uri, None)
            .map(|request| request.process())
            .unwrap_or_else(Response::Error),
        Method::POST => NotesRequest::parse(request.uri, Some(body))
            .map(|request| request.process())
            .unwrap_or_else(Response::Error),
        method => Response::Error(format!("Unsupported HTTP method {}", method)),
    };

    let response = serde_json::to_string(&response).unwrap_or_else(Response::json_error_from);
    http::Response::new(response.into_bytes())
}

#[derive(Debug, Error)]
enum NoteError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("File name is not valid utf-8 string")]
    WrongFileName,
}

impl From<NoteError> for Response {
    fn from(err: NoteError) -> Self {
        Response::Error(format!("{}", err))
    }
}

enum NotesRequest {
    GetNotes,
    GetNote(String),
    UpdateNote(String, String),
    RenameNote(String, String),
    DeleteNote(String),
}

impl NotesRequest {
    fn parse(uri: Uri, body: Option<Vec<u8>>) -> Result<Self, String> {
        let path = uri.path();
        let chunks: Vec<_> = path.split(|c| c == '/').collect();

        match &chunks[..] {
            [.., "list"] => Ok(Self::GetNotes),
            [.., "note", name] => {
                if let Some(body) = body {
                    let content = String::from_utf8(body).map_err(|err| err.to_string())?;
                    Ok(Self::UpdateNote(name.to_string(), content))
                } else {
                    Ok(Self::GetNote(name.to_string()))
                }
            },
            [.., "rename", name] => {
                if let Some(body) = body {
                    let content = String::from_utf8(body).map_err(|err| err.to_string())?;
                    Ok(Self::RenameNote(name.to_string(), content.trim().to_string()))
                } else {
                    Err(format!("New name for '{}' not specified", name))
                }
            },
            [.., "delete", name] => Ok(Self::DeleteNote(name.to_string())),
            _ => Err(format!("Cannot parse uri path {}, {:?}", path, chunks)),
        }
    }

    fn process(self) -> Response {
        match self {
            Self::GetNotes => process_notes().map(Response::Notes),
            Self::GetNote(name) => process_note(name.as_str()).map(Response::Note),
            Self::UpdateNote(name, content) => process_update(name.as_str(), content).map(Response::Note),
            Self::RenameNote(name, new_name) => process_rename(name.as_str(), new_name.as_str()).map(Response::Notes),
            Self::DeleteNote(name) => process_delete(name.as_str()).map(Response::Notes),
        }
        .unwrap_or_else(Response::from)
    }
}

fn process_notes() -> Result<Vec<Note>, NoteError> {
    let mut notes = vec![];

    for entry in dir_entries()? {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_file() {
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| NoteError::WrongFileName)?
                    .trim_end_matches(".md")
                    .to_string();

                let file = File::open(entry.path())?;
                let reader = BufReader::new(file);
                let preview = make_preview(reader.lines())?;

                notes.push(Note {
                    name,
                    content: NoteContent::Preview(preview),
                })
            }
        }
    }
    Ok(notes)
}

fn process_note(name: &str) -> Result<Note, NoteError> {
    let path = Path::new("/").join(format!("{}.md", name));
    let content = fs::read_to_string(path)?;
    Ok(Note {
        name: name.to_string(),
        content: NoteContent::FullBody(content),
    })
}

fn process_update(name: &str, content: String) -> Result<Note, NoteError> {
    let path = Path::new("/").join(format!("{}.md", name));

    fs::write(path, content)?;
    process_note(name)
}

fn process_delete(name: &str) -> Result<Vec<Note>, NoteError> {
    let path = Path::new("/").join(format!("{}.md", name));

    fs::remove_file(path)?;
    process_notes()
}

fn process_rename(name: &str, new_name: &str) -> Result<Vec<Note>, NoteError> {
    let from_path = Path::new("/").join(format!("{}.md", name));
    let to_path = Path::new("/").join(format!("{}.md", new_name));

    fs::rename(from_path, to_path)?;
    process_notes()
}

fn dir_entries() -> io::Result<Vec<DirEntry>> {
    fs::read_dir("/")?.collect()
}

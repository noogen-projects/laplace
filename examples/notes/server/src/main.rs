#![no_main]

use std::{
    fs::{self, DirEntry, File},
    io::{self, BufRead, BufReader},
    path::Path,
};

use dapla_wasm::WasmSlice;
use notes_common::{make_preview, Note, NoteContent, Response};
use thiserror::Error;

#[no_mangle]
pub extern "C" fn get(uri_ptr: *const u8, uri_len: usize) -> WasmSlice {
    static mut RESULT: String = String::new();

    let uri = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(uri_ptr, uri_len)) };
    let response = NotesRequest::parse(uri, None)
        .map(|request| request.process())
        .unwrap_or_else(Response::Error);
    let result = serde_json::to_string(&response).unwrap_or_else(Response::json_error_from);

    unsafe {
        RESULT = result;
        WasmSlice::from(RESULT.as_str())
    }
}

#[no_mangle]
pub extern "C" fn post(uri_ptr: *const u8, uri_len: usize, body_ptr: *const u8, body_len: usize) -> WasmSlice {
    static mut RESULT: String = String::new();

    let uri = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(uri_ptr, uri_len)) };
    let content = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(body_ptr, body_len)) };
    let response = NotesRequest::parse(uri, Some(content))
        .map(|request| request.process())
        .unwrap_or_else(Response::Error);
    let result = serde_json::to_string(&response).unwrap_or_else(Response::json_error_from);

    unsafe {
        RESULT = result;
        WasmSlice::from(RESULT.as_str())
    }
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
    DeleteNote(String),
}

impl NotesRequest {
    fn parse(uri: &str, content: Option<&str>) -> Result<Self, String> {
        let chunks: Vec<_> = uri.split(|c| c == '/').collect();
        match &chunks[..] {
            [.., "list"] => Ok(Self::GetNotes),
            [.., "note", name] => Ok(if let Some(content) = content {
                Self::UpdateNote(name.to_string(), content.to_string())
            } else {
                Self::GetNote(name.to_string())
            }),
            [.., "delete", name] => Ok(Self::DeleteNote(name.to_string())),
            _ => Err(format!("Cannot parse uri {}, {:?}", uri, chunks)),
        }
    }

    fn process(self) -> Response {
        match self {
            Self::GetNotes => process_notes().map(Response::Notes),
            Self::GetNote(name) => process_note(name.as_str()).map(Response::Note),
            Self::UpdateNote(name, content) => process_update(name.as_str(), content).map(Response::Note),
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

fn dir_entries() -> io::Result<Vec<DirEntry>> {
    fs::read_dir("/")?.collect()
}

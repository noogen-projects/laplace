#![no_main]

use std::{
    fs::{self, DirEntry, File},
    io::{self, BufRead, BufReader},
    iter,
    path::Path,
};

use dapla_wasm::WasmSlice;
use notes_common::{Note, NoteContent, Response};
use thiserror::Error;

#[no_mangle]
pub extern "C" fn get(uri_ptr: *const u8, uri_len: usize) -> WasmSlice {
    static mut RESULT: String = String::new();

    let uri = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(uri_ptr, uri_len)) };
    let response = get_notes(uri);
    let result = serde_json::to_string(&response).unwrap_or_else(Response::json_error_from);

    unsafe {
        RESULT = result;
        WasmSlice::from(RESULT.as_str())
    }
}

fn get_notes(uri: &str) -> Response {
    RequestOfGet::parse(uri)
        .map(|request| request.process())
        .unwrap_or_else(Response::Error)
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

enum RequestOfGet {
    Notes,
    Note(String),
}

impl RequestOfGet {
    fn parse(uri: &str) -> Result<Self, String> {
        let chunks: Vec<_> = uri.split(|c| c == '/').collect();
        match &chunks[..] {
            [.., "list"] => Ok(Self::Notes),
            [.., "note", name] => Ok(Self::Note(name.to_string())),
            _ => Err(format!("Cannot parse uri {}, {:?}", uri, chunks)),
        }
    }

    fn process(&self) -> Response {
        match self {
            Self::Notes => process_notes().map(Response::Notes),
            Self::Note(name) => process_note(name.as_str()).map(Response::Note),
        }
        .unwrap_or_else(Response::from)
    }
}

fn process_notes() -> Result<Vec<Note>, NoteError> {
    const PREVIEW_LIMIT: usize = 300;

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

                let mut preview = String::new();
                let mut preview_chars = 0;

                let mut prev_line = String::new();
                'lines: for line in reader.lines() {
                    let line = line?;
                    if line.starts_with("---") && prev_line.is_empty() {
                        break 'lines;
                    }

                    for ch in line.chars().chain(iter::once('\n')) {
                        preview.push(ch);
                        preview_chars += 1;
                        if preview_chars >= PREVIEW_LIMIT {
                            break 'lines;
                        }
                    }
                    prev_line = line;
                }

                if preview.ends_with("\n\n") {
                    preview.remove(preview.len() - 1);
                }

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

fn dir_entries() -> io::Result<Vec<DirEntry>> {
    fs::read_dir("/")?.collect()
}

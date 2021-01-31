#![no_main]

use dapla_wasm::WasmSlice;
use std::{convert::identity, fmt, fs, io, path::PathBuf};

#[no_mangle]
pub extern "C" fn get(_uri_ptr: *const u8, _uri_len: usize) -> WasmSlice {
    static mut RESULT: String = String::new();

    let files = read_files()
        .map_err(to_error_json)
        .and_then(|files| serde_json::to_string(&files).map_err(to_error_json))
        .unwrap_or_else(identity);

    unsafe {
        RESULT = files;
        WasmSlice::from(RESULT.as_str())
    }
}

fn read_files() -> io::Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir("/")?
        .map(|res| res.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
    entries.sort();

    Ok(entries)
}

fn to_error_json<E: fmt::Debug>(err: E) -> String {
    format!(r#"{{"error":"{:?}"}}"#, err)
}

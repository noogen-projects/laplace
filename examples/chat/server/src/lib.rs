use dapla_wasm::WasmSlice;
pub use dapla_wasm::{alloc, dealloc};

#[no_mangle]
pub unsafe extern "C" fn get(uri: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_get(uri.into_string()))
}

fn do_get(uri: String) -> String {
    let mut response = String::from("Echo ");
    response.push_str(&uri);
    response
}

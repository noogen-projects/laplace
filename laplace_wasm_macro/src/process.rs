use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

pub fn http(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let function = parse_macro_input!(input as ItemFn);
    let function_name = function.sig.ident.clone();
    let attrs = proc_macro2::TokenStream::from(attrs);

    let expanded = quote! {
        #[no_mangle]
        pub unsafe extern "C" fn process_http(request: ::laplace_wasm::WasmSlice) -> ::laplace_wasm::WasmSlice {
            use ::laplace_wasm::borsh::{BorshDeserialize, to_vec};
            use ::laplace_wasm::http;

            let mut request = request.into_vec_in_wasm();
            let request: http::Request = BorshDeserialize::deserialize(&mut request.as_slice())
                    .expect("HTTP request should be deserializable");
            let response: http::Response = #function_name(request);
            ::laplace_wasm::WasmSlice::from(
                to_vec(&response).expect("HTTP response should be serializable")
            )
        }

        #attrs
        #function
    };

    TokenStream::from(expanded)
}

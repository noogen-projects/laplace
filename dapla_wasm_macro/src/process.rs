use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

pub fn http(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let function = parse_macro_input!(input as ItemFn);
    let function_name = function.sig.ident.clone();
    let attrs = proc_macro2::TokenStream::from(attrs);

    let expanded = quote! {
        #[no_mangle]
        pub unsafe extern "C" fn process_http(request: ::dapla_wasm::WasmSlice) -> ::dapla_wasm::WasmSlice {
            use ::dapla_wasm::{borsh::{BorshDeserialize, BorshSerialize}, http};

            let mut request = request.into_vec_in_wasm();
            let request: http::Request = BorshDeserialize::deserialize(&mut request.as_slice())
                    .expect("HTTP request should be deserializable");
            let response: http::Response = #function_name(request);
            ::dapla_wasm::WasmSlice::from(
                response.try_to_vec().expect("HTTP response should be serializable")
            )
        }

        #attrs
        #function
    };

    TokenStream::from(expanded)
}

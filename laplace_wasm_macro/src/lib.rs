use proc_macro::TokenStream;

mod process;

#[proc_macro_attribute]
pub fn process_http(attrs: TokenStream, input: TokenStream) -> TokenStream {
    process::http(attrs, input)
}

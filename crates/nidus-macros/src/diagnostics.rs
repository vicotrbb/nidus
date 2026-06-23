use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Error;

pub(crate) fn compile_error(message: impl AsRef<str>) -> TokenStream {
    Error::new(Span::call_site(), message.as_ref())
        .to_compile_error()
        .into()
}

pub(crate) fn compile_error_with_item(message: impl AsRef<str>, item: TokenStream) -> TokenStream {
    let error = compile_error(message);
    quote! {
        #error
        #item
    }
}

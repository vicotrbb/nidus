use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Error;

pub(crate) fn compile_error(message: impl AsRef<str>) -> TokenStream {
    compile_error_at(Span::call_site(), message)
}

pub(crate) fn compile_error_at(span: Span, message: impl AsRef<str>) -> TokenStream {
    Error::new(span, message.as_ref()).to_compile_error()
}

pub(crate) fn compile_error_with_item(message: impl AsRef<str>, item: TokenStream) -> TokenStream {
    let error = compile_error(message);
    quote! {
        #error
        #item
    }
}

pub(crate) fn compile_error_with_item_at(
    span: Span,
    message: impl AsRef<str>,
    item: TokenStream,
) -> TokenStream {
    let error = compile_error_at(span, message);
    quote! {
        #error
        #item
    }
}

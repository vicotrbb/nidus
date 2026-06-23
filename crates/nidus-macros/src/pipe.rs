use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItemFn, Path, parse2};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if parse2::<Path>(attr).is_err() {
        return crate::diagnostics::compile_error_with_item(
            "#[pipe] requires a pipe type like #[pipe(ValidationPipe)]",
            item,
        );
    }

    if parse2::<ImplItemFn>(item.clone()).is_err() {
        return crate::diagnostics::compile_error_with_item(
            "#[pipe] can only be used on route methods",
            item,
        );
    }

    quote!(#item)
}

pub(crate) fn expand_validate(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return crate::diagnostics::compile_error_with_item(
            "#[validate] does not accept arguments",
            item,
        );
    }

    if parse2::<ImplItemFn>(item.clone()).is_err() {
        return crate::diagnostics::compile_error_with_item(
            "#[validate] can only be used on route methods",
            item,
        );
    }

    quote!(#item)
}

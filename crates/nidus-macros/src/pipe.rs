use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItemFn, Path, parse2};

use crate::utils::require_method_receiver;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if parse2::<Path>(attr).is_err() {
        return crate::diagnostics::compile_error_with_item(
            "#[pipe] requires a pipe type like #[pipe(ValidationPipe)]",
            item,
        );
    }

    match parse2::<ImplItemFn>(item.clone()) {
        Ok(function) => match require_method_receiver(&function, "pipe") {
            Ok(()) => quote!(#function),
            Err(error) => crate::diagnostics::compile_error_with_item(error.to_string(), item),
        },
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[pipe] can only be used on route methods: {error}"),
            item,
        ),
    }
}

pub(crate) fn expand_validate(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return crate::diagnostics::compile_error_with_item(
            "#[validate] does not accept arguments",
            item,
        );
    }

    match parse2::<ImplItemFn>(item.clone()) {
        Ok(function) => match require_method_receiver(&function, "validate") {
            Ok(()) => quote!(#function),
            Err(error) => crate::diagnostics::compile_error_with_item(error.to_string(), item),
        },
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[validate] can only be used on route methods: {error}"),
            item,
        ),
    }
}

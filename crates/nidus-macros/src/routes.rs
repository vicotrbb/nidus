use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItemFn, ItemImpl, parse2};

use crate::utils::{require_empty_attr, require_path_attr};

pub(crate) fn expand_routes(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = require_empty_attr(attr, "routes") {
        return error;
    }

    match parse2::<ItemImpl>(item.clone()) {
        Ok(item) => quote!(#item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[routes] can only be used on impl blocks: {error}"),
            item,
        ),
    }
}

pub(crate) fn expand_route(name: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    if require_path_attr(attr, name).is_err() {
        return crate::diagnostics::compile_error_with_item(
            format!("#[{name}] requires a string literal path like #[{name}(\"/:id\")]"),
            item,
        );
    }

    match parse2::<ImplItemFn>(item.clone()) {
        Ok(item) => quote!(#item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[{name}] can only be used on methods inside #[routes] impl blocks: {error}"),
            item,
        ),
    }
}

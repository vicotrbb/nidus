use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse2};

use crate::utils::require_empty_attr;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = require_empty_attr(attr, "module") {
        return error;
    }

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => quote!(#item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[module] can only be used on structs: {error}"),
            item,
        ),
    }
}

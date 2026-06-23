use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse2};

use crate::utils::require_path_attr;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = require_path_attr(attr, "controller") {
        return error;
    }

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => quote!(#item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[controller] can only be used on structs: {error}"),
            item,
        ),
    }
}

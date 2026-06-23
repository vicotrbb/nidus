use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, parse2};

use crate::utils::require_path_attr;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let prefix = match require_path_attr(attr, "controller") {
        Ok(prefix) => prefix,
        Err(error) => return error,
    };

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => {
            let name = &item.ident;
            quote! {
                #item

                impl #name {
                    pub const fn controller_prefix() -> &'static str {
                        #prefix
                    }
                }
            }
        }
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[controller] can only be used on structs: {error}"),
            item,
        ),
    }
}

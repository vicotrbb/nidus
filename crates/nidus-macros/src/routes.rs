use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItem, ImplItemFn, ItemImpl, LitStr, parse2};

use crate::utils::{require_empty_attr, require_path_attr};

pub(crate) fn expand_routes(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = require_empty_attr(attr, "routes") {
        return error;
    }

    match parse2::<ItemImpl>(item.clone()) {
        Ok(item) => expand_routes_impl(item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[routes] can only be used on impl blocks: {error}"),
            item,
        ),
    }
}

fn expand_routes_impl(item: ItemImpl) -> TokenStream {
    let self_ty = &item.self_ty;
    let metadata = item
        .items
        .iter()
        .filter_map(route_metadata)
        .collect::<Vec<_>>();
    let methods = metadata.iter().map(|(method, _path)| method);
    let paths = metadata.iter().map(|(_method, path)| path);

    quote! {
        #item

        impl #self_ty {
            pub fn routes() -> ::std::vec::Vec<::nidus::prelude::RouteMetadata> {
                ::std::vec![
                    #(::nidus::prelude::RouteMetadata::new(#methods, #paths),)*
                ]
            }
        }
    }
}

fn route_metadata(item: &ImplItem) -> Option<(String, LitStr)> {
    let ImplItem::Fn(function) = item else {
        return None;
    };

    for attr in &function.attrs {
        for (name, method) in [
            ("get", "GET"),
            ("post", "POST"),
            ("put", "PUT"),
            ("patch", "PATCH"),
            ("delete", "DELETE"),
        ] {
            if attr.path().is_ident(name) {
                return attr
                    .parse_args::<LitStr>()
                    .ok()
                    .map(|path| (method.to_owned(), path));
            }
        }
    }

    None
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

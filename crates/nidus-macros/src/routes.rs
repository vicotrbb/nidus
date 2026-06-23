use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Expr, ImplItem, ImplItemFn, ItemImpl, Lit, LitStr, MetaNameValue, Token, parse2,
    punctuated::Punctuated,
};

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
    let route_entries = metadata.iter().map(|route| {
        let method = &route.method;
        let path = &route.path;
        match &route.summary {
            Some(summary) => {
                quote!(::nidus::prelude::RouteMetadata::with_summary(#method, #path, #summary))
            }
            None => quote!(::nidus::prelude::RouteMetadata::new(#method, #path)),
        }
    });

    quote! {
        #item

        impl #self_ty {
            pub fn routes() -> ::std::vec::Vec<::nidus::prelude::RouteMetadata> {
                ::std::vec![
                    #(#route_entries,)*
                ]
            }
        }
    }
}

struct RouteMacroMetadata {
    method: String,
    path: LitStr,
    summary: Option<LitStr>,
}

fn route_metadata(item: &ImplItem) -> Option<RouteMacroMetadata> {
    let ImplItem::Fn(function) = item else {
        return None;
    };

    let summary = openapi_summary(function);
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
                    .map(|path| RouteMacroMetadata {
                        method: method.to_owned(),
                        path,
                        summary: summary.clone(),
                    });
            }
        }
    }

    None
}

fn openapi_summary(function: &ImplItemFn) -> Option<LitStr> {
    function
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("openapi"))
        .and_then(parse_openapi_summary)
}

fn parse_openapi_summary(attr: &syn::Attribute) -> Option<LitStr> {
    let args = attr
        .parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)
        .ok()?;
    args.into_iter().find_map(|arg| {
        if !arg.path.is_ident("summary") {
            return None;
        }
        let Expr::Lit(expr_lit) = arg.value else {
            return None;
        };
        let Lit::Str(summary) = expr_lit.lit else {
            return None;
        };
        Some(summary)
    })
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

pub(crate) fn expand_openapi(attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed = parse2::<ImplItemFn>(item.clone());
    let Ok(function) = parsed else {
        return crate::diagnostics::compile_error_with_item(
            "#[openapi] can only be used on route methods",
            item,
        );
    };

    let attribute = syn::parse_quote!(#[openapi(#attr)]);
    if parse_openapi_summary(&attribute).is_none() {
        return crate::diagnostics::compile_error_with_item(
            "#[openapi] requires summary = \"...\" metadata",
            quote!(#function),
        );
    }

    quote!(#function)
}

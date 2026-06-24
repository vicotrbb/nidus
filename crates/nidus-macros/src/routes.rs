use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItem, ImplItemFn, ItemImpl, LitStr, Path, parse2};

use crate::routes_openapi::openapi_metadata;
use crate::utils::{require_empty_attr, require_path_attr, validate_route_path};

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
    let mut metadata = Vec::new();
    let mut errors = Vec::new();
    for item in &item.items {
        match route_metadata(item) {
            Ok(Some(route)) => metadata.push(route),
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        let compile_errors = errors.iter().map(syn::Error::to_compile_error);
        return quote! {
            #item
            #(#compile_errors)*
        };
    }
    let route_entries = metadata.iter().map(|route| {
        let method = &route.method;
        let path = &route.path;
        let summary = match &route.summary {
            Some(summary) => quote!(::std::option::Option::Some(#summary)),
            None => quote!(::std::option::Option::None),
        };
        let tags = &route.tags;
        let response_status = match route.response_status {
            Some(status) => quote!(
                ::std::option::Option::Some(
                    ::nidus::prelude::StatusCode::from_u16(#status)
                        .expect("#[openapi] status metadata was validated by nidus-macros")
                )
            ),
            None => quote!(::std::option::Option::None),
        };
        let request_schema = match &route.request_schema {
            Some(schema) => quote!(::std::option::Option::Some(#schema)),
            None => quote!(::std::option::Option::None),
        };
        let response_schema = match &route.response_schema {
            Some(schema) => quote!(::std::option::Option::Some(#schema)),
            None => quote!(::std::option::Option::None),
        };
        let guards = &route.guards;
        let pipes = &route.pipes;
        let validates = route.validates;

        quote! {
            ::nidus::prelude::RouteMetadata::with_openapi_annotations(
                #method,
                #path,
                #summary,
                &[#(#tags,)*],
                &[#(::std::stringify!(#guards),)*],
                &[#(::std::stringify!(#pipes),)*],
                #validates,
            )
            .with_openapi_status(#response_status)
            .with_openapi_schemas(#request_schema, #response_schema)
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
    tags: Vec<LitStr>,
    response_status: Option<u16>,
    request_schema: Option<LitStr>,
    response_schema: Option<LitStr>,
    guards: Vec<Path>,
    pipes: Vec<Path>,
    validates: bool,
}

fn route_metadata(item: &ImplItem) -> syn::Result<Option<RouteMacroMetadata>> {
    let ImplItem::Fn(function) = item else {
        return Ok(None);
    };

    let openapi = match openapi_metadata(function) {
        Ok(openapi) => openapi,
        Err(_) => return Ok(None),
    };
    let summary = openapi.as_ref().map(|metadata| metadata.summary.clone());
    let (tags, response_status, request_schema, response_schema) = openapi
        .map(|metadata| {
            (
                metadata.tags,
                metadata.response_status,
                metadata.request_schema,
                metadata.response_schema,
            )
        })
        .unwrap_or_else(|| (Vec::new(), None, None, None));
    let guards = type_attributes(function, "guard");
    let pipes = type_attributes(function, "pipe");
    let validates = function
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("validate"));
    let route_attrs = function
        .attrs
        .iter()
        .filter_map(|attr| {
            [
                ("get", "GET"),
                ("post", "POST"),
                ("put", "PUT"),
                ("patch", "PATCH"),
                ("delete", "DELETE"),
            ]
            .into_iter()
            .find(|(name, _method)| attr.path().is_ident(name))
            .map(|(_name, method)| (attr, method))
        })
        .collect::<Vec<_>>();

    if route_attrs.len() > 1 {
        return Err(syn::Error::new_spanned(
            function.sig.ident.clone(),
            "route methods must declare exactly one HTTP method attribute",
        ));
    }

    let Some((attr, method)) = route_attrs.first() else {
        if has_route_metadata_attributes(function) {
            return Err(syn::Error::new_spanned(
                function.sig.ident.clone(),
                "route metadata attributes require an HTTP method attribute",
            ));
        }
        return Ok(None);
    };

    let Ok(path) = attr.parse_args::<LitStr>() else {
        return Ok(None);
    };
    if validate_route_path(&path).is_err() {
        return Ok(None);
    }
    Ok(Some(RouteMacroMetadata {
        method: (*method).to_owned(),
        path,
        summary,
        tags,
        response_status,
        request_schema,
        response_schema,
        guards,
        pipes,
        validates,
    }))
}

fn has_route_metadata_attributes(function: &ImplItemFn) -> bool {
    function.attrs.iter().any(|attr| {
        attr.path().is_ident("guard")
            || attr.path().is_ident("pipe")
            || attr.path().is_ident("validate")
            || attr.path().is_ident("openapi")
    })
}

fn type_attributes(function: &ImplItemFn, name: &str) -> Vec<Path> {
    function
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident(name))
        .filter_map(|attr| attr.parse_args::<Path>().ok())
        .collect()
}

pub(crate) fn expand_route(name: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = match require_path_attr(attr, name) {
        Ok(path) => path,
        Err(_) => {
            return crate::diagnostics::compile_error_with_item(
                format!("#[{name}] requires a string literal path like #[{name}(\"/:id\")]"),
                item,
            );
        }
    };
    if let Err(error) = validate_route_path(&path) {
        return crate::diagnostics::compile_error_with_item(error.to_string(), item);
    }

    match parse2::<ImplItemFn>(item.clone()) {
        Ok(item) => quote!(#item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[{name}] can only be used on methods inside #[routes] impl blocks: {error}"),
            item,
        ),
    }
}

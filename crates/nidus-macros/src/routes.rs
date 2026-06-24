use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Expr, ImplItem, ImplItemFn, ItemImpl, Lit, LitInt, LitStr, MetaNameValue, Path, PathArguments,
    Token, parse2, punctuated::Punctuated,
};

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

struct OpenApiMetadata {
    summary: LitStr,
    tags: Vec<LitStr>,
    response_status: Option<u16>,
    request_schema: Option<LitStr>,
    response_schema: Option<LitStr>,
}

fn openapi_metadata(function: &ImplItemFn) -> syn::Result<Option<OpenApiMetadata>> {
    let Some(attr) = function
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("openapi"))
    else {
        return Ok(None);
    };

    parse_openapi_metadata(attr).map(Some)
}

fn parse_openapi_metadata(attr: &syn::Attribute) -> syn::Result<OpenApiMetadata> {
    let args = attr.parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)?;
    let mut summary = None;
    let mut tags = Vec::new();
    let mut response_status = None;
    let mut request_schema = None;
    let mut response_schema = None;

    for arg in args {
        if arg.path.is_ident("summary") {
            let Expr::Lit(expr_lit) = arg.value else {
                return Err(syn::Error::new_spanned(
                    arg,
                    "#[openapi] summary must be a string literal",
                ));
            };
            let Lit::Str(value) = expr_lit.lit else {
                return Err(syn::Error::new_spanned(
                    expr_lit,
                    "#[openapi] summary must be a string literal",
                ));
            };
            summary = Some(value);
        } else if arg.path.is_ident("tags") {
            let Expr::Array(array) = arg.value else {
                return Err(syn::Error::new_spanned(
                    arg,
                    "#[openapi] tags must be an array of string literals",
                ));
            };
            for element in array.elems {
                let Expr::Lit(expr_lit) = element else {
                    return Err(syn::Error::new_spanned(
                        element,
                        "#[openapi] tags must be string literals",
                    ));
                };
                let Lit::Str(tag) = expr_lit.lit else {
                    return Err(syn::Error::new_spanned(
                        expr_lit,
                        "#[openapi] tags must be string literals",
                    ));
                };
                tags.push(tag);
            }
        } else if arg.path.is_ident("request") {
            request_schema = Some(schema_name(&arg.value, "request")?);
        } else if arg.path.is_ident("response") {
            response_schema = Some(schema_name(&arg.value, "response")?);
        } else if arg.path.is_ident("status") {
            response_status = Some(response_status_code(&arg.value)?);
        } else {
            return Err(syn::Error::new_spanned(
                arg.path,
                "#[openapi] supports only summary = \"...\", tags = [\"...\"], status = 201, request = Type, and response = Type metadata",
            ));
        }
    }

    let Some(summary) = summary else {
        return Err(syn::Error::new_spanned(
            attr,
            "#[openapi] requires summary = \"...\" metadata",
        ));
    };

    Ok(OpenApiMetadata {
        summary,
        tags,
        response_status,
        request_schema,
        response_schema,
    })
}

fn schema_name(value: &Expr, name: &str) -> syn::Result<LitStr> {
    let Expr::Path(expr_path) = value else {
        return Err(syn::Error::new_spanned(
            value,
            format!("#[openapi] {name} must be a type path"),
        ));
    };
    let Some(segment) = expr_path.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            value,
            format!("#[openapi] {name} must be a type path"),
        ));
    };
    if expr_path
        .path
        .segments
        .iter()
        .any(|segment| !matches!(segment.arguments, PathArguments::None))
    {
        return Err(syn::Error::new_spanned(
            value,
            format!("#[openapi] {name} must be a type path"),
        ));
    }
    Ok(LitStr::new(
        &segment.ident.to_string(),
        segment.ident.span(),
    ))
}

fn response_status_code(value: &Expr) -> syn::Result<u16> {
    let Expr::Lit(expr_lit) = value else {
        return Err(syn::Error::new_spanned(
            value,
            "#[openapi] status must be an HTTP status code integer literal",
        ));
    };
    let Lit::Int(status) = &expr_lit.lit else {
        return Err(syn::Error::new_spanned(
            expr_lit,
            "#[openapi] status must be an HTTP status code integer literal",
        ));
    };
    parse_status_literal(status)
}

fn parse_status_literal(status: &LitInt) -> syn::Result<u16> {
    let value = status.base10_parse::<u16>()?;
    if !(100..=599).contains(&value) {
        return Err(syn::Error::new_spanned(
            status,
            "#[openapi] status must be in the HTTP status code range 100..=599",
        ));
    }
    Ok(value)
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

pub(crate) fn expand_openapi(attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed = parse2::<ImplItemFn>(item.clone());
    let Ok(function) = parsed else {
        return crate::diagnostics::compile_error_with_item(
            "#[openapi] can only be used on route methods",
            item,
        );
    };

    let attribute = syn::parse_quote!(#[openapi(#attr)]);
    if let Err(error) = parse_openapi_metadata(&attribute) {
        return crate::diagnostics::compile_error_with_item(error.to_string(), quote!(#function));
    }

    quote!(#function)
}

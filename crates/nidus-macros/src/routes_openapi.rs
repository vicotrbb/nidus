use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Expr, ImplItemFn, Lit, LitInt, LitStr, MetaNameValue, PathArguments, Token, parse2,
    punctuated::Punctuated,
};

use crate::utils::require_method_receiver;

pub(crate) struct OpenApiMetadata {
    pub(crate) summary: LitStr,
    pub(crate) tags: Vec<LitStr>,
    pub(crate) response_status: Option<u16>,
    pub(crate) request_schema: Option<LitStr>,
    pub(crate) response_schema: Option<LitStr>,
}

pub(crate) fn openapi_metadata(function: &ImplItemFn) -> syn::Result<Option<OpenApiMetadata>> {
    let attrs = function
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("openapi"))
        .collect::<Vec<_>>();
    if attrs.is_empty() {
        return Ok(None);
    }
    if attrs.len() > 1 {
        return Err(syn::Error::new_spanned(
            function.sig.ident.clone(),
            "route methods can declare at most one #[openapi] attribute",
        ));
    }

    match parse_openapi_metadata(attrs[0]) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(_) => Ok(None),
    }
}

pub(crate) fn parse_openapi_metadata(attr: &syn::Attribute) -> syn::Result<OpenApiMetadata> {
    let args = attr.parse_args_with(Punctuated::<MetaNameValue, Token![,]>::parse_terminated)?;
    let mut summary = None;
    let mut tags = Vec::new();
    let mut response_status = None;
    let mut request_schema = None;
    let mut response_schema = None;

    for arg in args {
        if arg.path.is_ident("summary") {
            summary = Some(summary_literal(arg)?);
        } else if arg.path.is_ident("tags") {
            tags = tag_literals(arg)?;
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

pub(crate) fn expand_openapi(attr: TokenStream, item: TokenStream) -> TokenStream {
    let parsed = parse2::<ImplItemFn>(item.clone());
    let Ok(function) = parsed else {
        return crate::diagnostics::compile_error_with_item(
            "#[openapi] can only be used on route methods",
            item,
        );
    };
    if let Err(error) = require_method_receiver(&function, "openapi") {
        return crate::diagnostics::compile_error_with_item(error.to_string(), item);
    }

    let attribute = syn::parse_quote!(#[openapi(#attr)]);
    if let Err(error) = parse_openapi_metadata(&attribute) {
        return crate::diagnostics::compile_error_with_item(error.to_string(), quote!(#function));
    }

    quote!(#function)
}

fn summary_literal(arg: MetaNameValue) -> syn::Result<LitStr> {
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
    Ok(value)
}

fn tag_literals(arg: MetaNameValue) -> syn::Result<Vec<LitStr>> {
    let Expr::Array(array) = arg.value else {
        return Err(syn::Error::new_spanned(
            arg,
            "#[openapi] tags must be an array of string literals",
        ));
    };

    let mut tags = Vec::new();
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
    Ok(tags)
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

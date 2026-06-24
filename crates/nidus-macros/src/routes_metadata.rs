use quote::format_ident;
use syn::{FnArg, Ident, ImplItem, ImplItemFn, LitStr, Path, Type};

use crate::routes_openapi::openapi_metadata;
use crate::utils::validate_route_path;

pub(crate) struct RouteMacroMetadata {
    pub(crate) function: Ident,
    pub(crate) method: String,
    pub(crate) path: LitStr,
    pub(crate) handler_args: Vec<HandlerArgument>,
    pub(crate) summary: Option<LitStr>,
    pub(crate) tags: Vec<LitStr>,
    pub(crate) response_status: Option<u16>,
    pub(crate) request_schema: Option<Path>,
    pub(crate) response_schema: Option<Path>,
    pub(crate) guards: Vec<Path>,
    pub(crate) pipes: Vec<Path>,
    pub(crate) validates: bool,
}

pub(crate) struct HandlerArgument {
    pub(crate) ident: Ident,
    pub(crate) ty: Box<Type>,
}

pub(crate) fn route_metadata(item: &ImplItem) -> syn::Result<Option<RouteMacroMetadata>> {
    let ImplItem::Fn(function) = item else {
        return Ok(None);
    };

    let openapi = openapi_metadata(function)?;
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
    validate_route_receiver(function)?;
    let handler_args = handler_arguments(function)?;
    Ok(Some(RouteMacroMetadata {
        function: function.sig.ident.clone(),
        method: (*method).to_owned(),
        path,
        handler_args,
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

fn validate_route_receiver(function: &ImplItemFn) -> syn::Result<()> {
    let Some(receiver) = function.sig.receiver() else {
        return Err(syn::Error::new_spanned(
            function.sig.ident.clone(),
            "route methods must take &self as their first argument",
        ));
    };

    if receiver.reference.is_none() || receiver.mutability.is_some() {
        return Err(syn::Error::new_spanned(
            receiver,
            "route methods must take &self; use interior shared state for mutation",
        ));
    }

    Ok(())
}

fn handler_arguments(function: &ImplItemFn) -> syn::Result<Vec<HandlerArgument>> {
    function
        .sig
        .inputs
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, argument)| match argument {
            FnArg::Typed(argument) => Ok(HandlerArgument {
                ident: format_ident!("__nidus_arg_{index}"),
                ty: argument.ty.clone(),
            }),
            FnArg::Receiver(receiver) => Err(syn::Error::new_spanned(
                receiver,
                "route methods must declare only one &self receiver",
            )),
        })
        .collect()
}

fn type_attributes(function: &ImplItemFn, name: &str) -> Vec<Path> {
    function
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident(name))
        .filter_map(|attr| attr.parse_args::<Path>().ok())
        .collect()
}

//! Procedural macros for Nidus modules, providers, controllers, and routes.

mod controller;
mod diagnostics;
mod guard;
mod injectable;
mod module;
mod pipe;
mod routes;
mod utils;

use proc_macro::TokenStream;

/// Declares a Nidus module.
#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    module::expand(attr.into(), item.into()).into()
}

/// Declares an injectable provider.
#[proc_macro_attribute]
pub fn injectable(attr: TokenStream, item: TokenStream) -> TokenStream {
    injectable::expand(attr.into(), item.into()).into()
}

/// Declares a controller path prefix.
#[proc_macro_attribute]
pub fn controller(attr: TokenStream, item: TokenStream) -> TokenStream {
    controller::expand(attr.into(), item.into()).into()
}

/// Validates a route implementation block.
#[proc_macro_attribute]
pub fn routes(attr: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand_routes(attr.into(), item.into()).into()
}

/// Declares a GET route.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand_route("get", attr.into(), item.into()).into()
}

/// Declares a POST route.
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand_route("post", attr.into(), item.into()).into()
}

/// Declares a PUT route.
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand_route("put", attr.into(), item.into()).into()
}

/// Declares a PATCH route.
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand_route("patch", attr.into(), item.into()).into()
}

/// Declares a DELETE route.
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand_route("delete", attr.into(), item.into()).into()
}

/// Declares a route guard.
#[proc_macro_attribute]
pub fn guard(attr: TokenStream, item: TokenStream) -> TokenStream {
    guard::expand(attr.into(), item.into()).into()
}

/// Declares a route pipe.
#[proc_macro_attribute]
pub fn pipe(attr: TokenStream, item: TokenStream) -> TokenStream {
    pipe::expand(attr.into(), item.into()).into()
}

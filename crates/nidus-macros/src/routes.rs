use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{ImplItemFn, ItemImpl, parse2};

use crate::routes_metadata::route_metadata;
use crate::utils::{
    require_empty_attr, require_method_receiver, require_path_attr, validate_route_path,
};

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
    let (impl_generics, _type_generics, where_clause) = item.generics.split_for_impl();
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
        let request_schema = schema_name(&route.request_schema);
        let response_schema = schema_name(&route.response_schema);
        let request_schema_registrar = schema_registrar(&route.request_schema);
        let response_schema_registrar = schema_registrar(&route.response_schema);
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
            .with_openapi_schema_registrars(
                #request_schema_registrar,
                #response_schema_registrar,
            )
        }
    });
    let route_definitions = metadata.iter().map(|route| {
        let function = &route.function;
        let route_constructor = format_ident!("try_{}", route.method.to_ascii_lowercase());
        let path = &route.path;
        let handler_args = route.handler_args.iter().map(|argument| {
            let ident = &argument.ident;
            let ty = &argument.ty;
            quote!(#ident: #ty)
        });
        let call_args = route.handler_args.iter().map(|argument| &argument.ident);

        quote! {
            {
                let __nidus_controller = ::std::sync::Arc::clone(&__nidus_controller);
                ::nidus::prelude::RouteDefinition::#route_constructor(
                    #path,
                    move |#(#handler_args),*| {
                        let __nidus_controller = ::std::sync::Arc::clone(&__nidus_controller);
                        async move {
                            __nidus_controller.#function(#(#call_args),*).await
                        }
                    },
                )?
            }
        }
    });
    let route_definitions_with_container = metadata.iter().map(|route| {
        let function = &route.function;
        let route_constructor = format_ident!("try_{}", route.method.to_ascii_lowercase());
        let path = &route.path;
        let handler_args = route
            .handler_args
            .iter()
            .map(|argument| {
                let ident = &argument.ident;
                let ty = &argument.ty;
                quote!(#ident: #ty)
            })
            .collect::<Vec<_>>();
        let call_args = route.handler_args.iter().map(|argument| &argument.ident);
        let guards = &route.guards;
        let label = format!(
            "{} {}::{}",
            route.method.to_ascii_uppercase(),
            path.value(),
            function
        );

        if guards.is_empty() {
            quote! {
                {
                    let __nidus_controller = ::std::sync::Arc::clone(&__nidus_controller);
                    ::nidus::prelude::RouteDefinition::#route_constructor(
                        #path,
                        move |#(#handler_args),*| {
                            let __nidus_controller = ::std::sync::Arc::clone(&__nidus_controller);
                            async move {
                                __nidus_controller.#function(#(#call_args),*).await
                            }
                        },
                    )
                    .map_err(|error| ::nidus::prelude::NidusError::ApplicationBuild {
                        message: error.to_string(),
                    })?
                }
            }
        } else {
            let guarded_handler_args = if handler_args.is_empty() {
                quote!(__nidus_headers: ::nidus::prelude::HeaderMap)
            } else {
                quote!(__nidus_headers: ::nidus::prelude::HeaderMap, #(#handler_args),*)
            };
            let guard_bindings = guards.iter().enumerate().map(|(index, guard)| {
                let ident = format_ident!("__nidus_guard_{index}");
                quote! {
                    let #ident = __nidus_container.resolve::<#guard>()?;
                }
            });
            let guard_clones = guards.iter().enumerate().map(|(index, _guard)| {
                let ident = format_ident!("__nidus_guard_{index}");
                let clone_ident = format_ident!("__nidus_guard_{index}");
                quote! {
                    let #clone_ident = ::std::sync::Arc::clone(&#ident);
                }
            });
            let guard_checks = guards.iter().enumerate().map(|(index, _guard)| {
                let ident = format_ident!("__nidus_guard_{index}");
                quote! {
                    if let Err(__nidus_error) = ::nidus::prelude::Guard::check(
                        #ident.as_ref(),
                        ::nidus::prelude::GuardContext::new((), #label)
                            .with_headers(__nidus_headers.clone()),
                    ).await {
                        return ::nidus::prelude::IntoResponse::into_response(__nidus_error);
                    }
                }
            });
            quote! {
                {
                    #(#guard_bindings)*
                    let __nidus_controller = ::std::sync::Arc::clone(&__nidus_controller);
                    ::nidus::prelude::RouteDefinition::#route_constructor(
                        #path,
                        move |#guarded_handler_args| {
                            let __nidus_controller = ::std::sync::Arc::clone(&__nidus_controller);
                            #(#guard_clones)*
                            async move {
                                #(#guard_checks)*
                                ::nidus::prelude::IntoResponse::into_response(
                                    __nidus_controller.#function(#(#call_args),*).await
                                )
                            }
                        },
                    )
                    .map_err(|error| ::nidus::prelude::NidusError::ApplicationBuild {
                        message: error.to_string(),
                    })?
                }
            }
        }
    });
    let router_methods = if metadata.is_empty() {
        quote!()
    } else {
        quote! {
            pub fn into_router(self) -> ::nidus::prelude::Router {
                self.try_into_router()
                    .unwrap_or_else(|error| panic!("{error}"))
            }

            pub fn try_into_router(
                self,
            ) -> ::std::result::Result<
                ::nidus::prelude::Router,
                ::nidus::prelude::RoutePathError,
            > {
                let __nidus_controller = ::std::sync::Arc::new(self);
                let __nidus_controller_routes =
                    ::nidus::prelude::Controller::try_new(Self::controller_prefix())?
                        #(.route(#route_definitions))*;
                __nidus_controller_routes.try_into_router()
            }

            pub fn try_into_router_with_container(
                self,
                __nidus_container: &::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<::nidus::prelude::Router> {
                let __nidus_controller = ::std::sync::Arc::new(self);
                let __nidus_controller_routes =
                    ::nidus::prelude::Controller::try_new(Self::controller_prefix())
                        .map_err(|error| ::nidus::prelude::NidusError::ApplicationBuild {
                            message: error.to_string(),
                        })?
                        #(.route(#route_definitions_with_container))*;
                __nidus_controller_routes
                    .try_into_router()
                    .map_err(|error| ::nidus::prelude::NidusError::ApplicationBuild {
                        message: error.to_string(),
                    })
            }
        }
    };
    let controller_registrant = if metadata.is_empty() {
        quote!()
    } else {
        quote! {
            impl #impl_generics ::nidus::prelude::ControllerRegistrant for #self_ty #where_clause {
                fn controller_name() -> &'static str {
                    ::std::any::type_name::<Self>().rsplit("::").next().unwrap()
                }

                fn controller_prefix() -> &'static str {
                    Self::controller_prefix()
                }

                fn build_router(
                    container: &::nidus::prelude::Container,
                ) -> ::nidus::prelude::Result<Box<dyn ::std::any::Any + Send + Sync>> {
                    Ok(Box::new(
                        Self::try_from_container(container)?
                            .try_into_router_with_container(container)?,
                    ))
                }

                fn route_metadata() -> Box<dyn ::std::any::Any + Send + Sync> {
                    Box::new(Self::routes())
                }
            }
        }
    };

    quote! {
        #item

        impl #impl_generics #self_ty #where_clause {
            pub fn routes() -> ::std::vec::Vec<::nidus::prelude::RouteMetadata> {
                ::std::vec![
                    #(#route_entries,)*
                ]
            }

            #router_methods
        }

        #controller_registrant
    }
}

fn schema_name(schema: &Option<syn::Path>) -> proc_macro2::TokenStream {
    let Some(schema) = schema else {
        return quote!(::std::option::Option::None);
    };
    let Some(segment) = schema.segments.last() else {
        return quote!(::std::option::Option::None);
    };
    let name = syn::LitStr::new(&segment.ident.to_string(), segment.ident.span());
    quote!(::std::option::Option::Some(#name))
}

fn schema_registrar(schema: &Option<syn::Path>) -> proc_macro2::TokenStream {
    let Some(schema) = schema else {
        return quote!(::std::option::Option::None);
    };
    quote!(
        ::std::option::Option::Some(
            |schemas| {
                ::nidus::register_openapi_schema::<#schema>(schemas);
            }
        )
    )
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
        Ok(function) => match require_method_receiver(&function, name) {
            Ok(()) => quote!(#function),
            Err(error) => crate::diagnostics::compile_error_with_item(error.to_string(), item),
        },
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[{name}] can only be used on methods inside #[routes] impl blocks: {error}"),
            item,
        ),
    }
}

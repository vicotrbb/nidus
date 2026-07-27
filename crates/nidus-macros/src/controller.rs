use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident, ItemStruct, parse2, spanned::Spanned};

use crate::utils::{DependencyWrapper, dependency_wrapper, require_path_attr, validate_route_path};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let prefix = match require_path_attr(attr, "controller") {
        Ok(prefix) => prefix,
        Err(error) => return error,
    };
    if let Err(error) = validate_route_path(&prefix) {
        return crate::diagnostics::compile_error_with_item(error.to_string(), item);
    }

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => {
            let name = &item.ident;
            let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
            let from_container = controller_from_container(&item);
            quote! {
                #item

                impl #impl_generics #name #type_generics #where_clause {
                    pub const fn controller_prefix() -> &'static str {
                        #prefix
                    }

                    #from_container
                }
            }
        }
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[controller] can only be used on structs: {error}"),
            item,
        ),
    }
}

fn controller_from_container(item: &ItemStruct) -> TokenStream {
    match &item.fields {
        Fields::Unit => quote! {
            pub fn try_from_container(
                _container: &::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<Self> {
                Ok(Self)
            }
        },
        Fields::Named(fields) => {
            let resolver = Ident::new("container", proc_macro2::Span::call_site());
            let initializers = match field_initializers(fields, &resolver) {
                Ok(initializers) => initializers,
                Err(error) => {
                    let message = error.to_string();
                    return quote! {
                        pub fn try_from_container(
                            _container: &::nidus::prelude::Container,
                        ) -> ::nidus::prelude::Result<Self> {
                            Err(::nidus::prelude::NidusError::ApplicationBuild {
                                message: #message.to_owned(),
                            })
                        }
                    };
                }
            };
            quote! {
                pub fn try_from_container(
                    #resolver: &::nidus::prelude::Container,
                ) -> ::nidus::prelude::Result<Self> {
                    Ok(Self {
                        #(#initializers,)*
                    })
                }
            }
        }
        Fields::Unnamed(fields) if fields.unnamed.is_empty() => quote! {
            pub fn try_from_container(
                _container: &::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<Self> {
                Ok(Self())
            }
        },
        Fields::Unnamed(_) => quote! {
            pub fn try_from_container(
                _container: &::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<Self> {
                Err(::nidus::prelude::NidusError::ApplicationBuild {
                    message: "#[controller] dependency instantiation supports unit structs and named fields using Inject<T> or Optional<T>".to_owned(),
                })
            }
        },
    }
}

fn field_initializers(
    fields: &syn::FieldsNamed,
    resolver: &Ident,
) -> syn::Result<Vec<TokenStream>> {
    fields
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.as_ref().expect("named field has ident");
            match dependency_wrapper(&field.ty) {
                Some(DependencyWrapper::Inject) => Ok(quote!(#ident: #resolver.inject()?)),
                Some(DependencyWrapper::Optional) => Ok(quote!(#ident: #resolver.optional()?)),
                None => Err(syn::Error::new(
                    field.ty.span(),
                    "#[controller] fields must use Inject<T> or Optional<T>; use app-specific construction for unsupported state",
                )),
            }
        })
        .collect()
}

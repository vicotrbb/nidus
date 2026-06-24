use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, GenericArgument, ItemStruct, PathArguments, Type, parse2};

use crate::utils::require_empty_attr;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = require_empty_attr(attr, "injectable") {
        return error;
    }

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => expand_struct(item),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[injectable] can only be used on structs: {error}"),
            item,
        ),
    }
}

fn expand_struct(item: ItemStruct) -> TokenStream {
    let name = &item.ident;
    let fields = match &item.fields {
        Fields::Named(fields) => fields,
        Fields::Unit => {
            return quote! {
                #item

                impl #name {
                    pub fn register_provider(
                        container: &mut ::nidus::prelude::Container,
                    ) -> ::nidus::prelude::Result<()> {
                        container.register_factory::<Self, _>(
                            ::nidus::prelude::ProviderLifetime::Singleton,
                            |_container| Ok(Self),
                        )
                    }
                }
            };
        }
        Fields::Unnamed(_) => {
            return crate::diagnostics::compile_error_with_item(
                "#[injectable] currently supports named-field structs or unit structs",
                quote!(#item),
            );
        }
    };

    let initializers = fields.named.iter().map(|field| {
        let ident = field.ident.as_ref().expect("named field has ident");
        match dependency_wrapper(&field.ty) {
            Some(DependencyWrapper::Inject) => quote!(#ident: container.inject()?),
            Some(DependencyWrapper::Optional) => quote!(#ident: container.optional()?),
            None => quote!(#ident: ::core::default::Default::default()),
        }
    });

    quote! {
        #item

        impl #name {
            pub fn register_provider(
                container: &mut ::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<()> {
                container.register_factory::<Self, _>(
                    ::nidus::prelude::ProviderLifetime::Singleton,
                    |container| {
                        Ok(Self {
                            #(#initializers,)*
                        })
                    },
                )
            }
        }
    }
}

enum DependencyWrapper {
    Inject,
    Optional,
}

fn dependency_wrapper(ty: &Type) -> Option<DependencyWrapper> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let has_type_argument = arguments
        .args
        .iter()
        .any(|argument| matches!(argument, GenericArgument::Type(_)));
    if !has_type_argument {
        return None;
    }
    match segment.ident.to_string().as_str() {
        "Inject" => Some(DependencyWrapper::Inject),
        "Optional" => Some(DependencyWrapper::Optional),
        _ => None,
    }
}

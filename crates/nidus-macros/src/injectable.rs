use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, GenericArgument, Ident, ItemStruct, PathArguments, Type, parse2};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let lifetime = match injectable_lifetime(attr) {
        Ok(lifetime) => lifetime,
        Err(error) => return crate::diagnostics::compile_error_with_item(error, item),
    };

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => expand_struct(item, lifetime),
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[injectable] can only be used on structs: {error}"),
            item,
        ),
    }
}

fn expand_struct(item: ItemStruct, lifetime: InjectableLifetime) -> TokenStream {
    let name = &item.ident;
    let provider_lifetime = lifetime.provider_lifetime_tokens();
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
                            #provider_lifetime,
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
                    #provider_lifetime,
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

enum InjectableLifetime {
    Singleton,
    Transient,
    Request,
}

impl InjectableLifetime {
    fn provider_lifetime_tokens(&self) -> TokenStream {
        match self {
            Self::Singleton => quote!(::nidus::prelude::ProviderLifetime::Singleton),
            Self::Transient => quote!(::nidus::prelude::ProviderLifetime::Transient),
            Self::Request => quote!(::nidus::prelude::ProviderLifetime::Request),
        }
    }
}

fn injectable_lifetime(attr: TokenStream) -> Result<InjectableLifetime, &'static str> {
    if attr.is_empty() {
        return Ok(InjectableLifetime::Singleton);
    }

    let Ok(ident) = parse2::<Ident>(attr) else {
        return Err("#[injectable] supports no arguments, singleton, transient, or request");
    };

    match ident.to_string().as_str() {
        "singleton" => Ok(InjectableLifetime::Singleton),
        "transient" => Ok(InjectableLifetime::Transient),
        "request" => Ok(InjectableLifetime::Request),
        _ => Err("#[injectable] supports no arguments, singleton, transient, or request"),
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

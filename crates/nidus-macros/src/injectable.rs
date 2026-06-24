use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Fields, GenericArgument, Ident, ItemStruct, PathArguments, Type, parse2, spanned::Spanned,
};

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
    let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();
    let fields = match &item.fields {
        Fields::Named(fields) => fields,
        Fields::Unit => {
            let register_provider = unit_register_provider(&lifetime);
            return quote! {
                #item

                impl #impl_generics #name #type_generics #where_clause {
                    pub fn register_provider(
                        container: &mut ::nidus::prelude::Container,
                    ) -> ::nidus::prelude::Result<()> {
                        #register_provider
                    }
                }

                impl #impl_generics ::nidus::prelude::ProviderRegistrant for #name #type_generics #where_clause {
                    fn register_provider(
                        container: &mut ::nidus::prelude::Container,
                    ) -> ::nidus::prelude::Result<()> {
                        Self::register_provider(container)
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

    let resolver = Ident::new("resolver", proc_macro2::Span::call_site());
    let initializers = match field_initializers(fields, &resolver) {
        Ok(initializers) => initializers,
        Err(error) => {
            let compile_error = error.to_compile_error();
            return quote! {
                #item
                #compile_error
            };
        }
    };
    let register_provider = named_register_provider(
        &lifetime,
        &resolver,
        quote! {
            Ok(Self {
                #(#initializers,)*
            })
        },
    );

    quote! {
        #item

        impl #impl_generics #name #type_generics #where_clause {
            pub fn register_provider(
                container: &mut ::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<()> {
                #register_provider
            }
        }

        impl #impl_generics ::nidus::prelude::ProviderRegistrant for #name #type_generics #where_clause {
            fn register_provider(
                container: &mut ::nidus::prelude::Container,
            ) -> ::nidus::prelude::Result<()> {
                Self::register_provider(container)
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

fn unit_register_provider(lifetime: &InjectableLifetime) -> TokenStream {
    match lifetime {
        InjectableLifetime::Request => {
            quote!(container.register_request_scoped::<Self, _>(|_scope| Ok(Self)))
        }
        InjectableLifetime::Singleton | InjectableLifetime::Transient => {
            let provider_lifetime = lifetime.provider_lifetime_tokens();
            quote! {
                container.register_factory::<Self, _>(
                    #provider_lifetime,
                    |_container| Ok(Self),
                )
            }
        }
    }
}

fn named_register_provider(
    lifetime: &InjectableLifetime,
    resolver: &Ident,
    body: TokenStream,
) -> TokenStream {
    match lifetime {
        InjectableLifetime::Request => {
            quote!(container.register_request_scoped::<Self, _>(|#resolver| #body))
        }
        InjectableLifetime::Singleton | InjectableLifetime::Transient => {
            let provider_lifetime = lifetime.provider_lifetime_tokens();
            quote! {
                container.register_factory::<Self, _>(
                    #provider_lifetime,
                    |#resolver| #body,
                )
            }
        }
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
                    "#[injectable] fields must use Inject<T> or Optional<T>; use an explicit factory for literal or default state",
                )),
        }
        })
        .collect()
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

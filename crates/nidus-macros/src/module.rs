use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Attribute, Generics, Ident, Path, Token, Visibility, braced, bracketed, parenthesized,
    parse::Parse, parse::ParseStream, parse2,
};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut metadata = match parse2::<ModuleMetadata>(attr) {
        Ok(metadata) => metadata,
        Err(error) => {
            return crate::diagnostics::compile_error_with_item(
                format!(
                    "#[module] expects groups like providers(UsersService), controllers(UsersController): {error}"
                ),
                item,
            );
        }
    };

    match parse2::<ModuleItem>(item.clone()) {
        Ok(item) => {
            metadata.extend(item.metadata);
            let name = &item.ident;
            let module_name = name.to_string();
            let imports = metadata.imports.iter().map(path_to_string);
            let providers = metadata.providers.iter().map(path_to_string);
            let controllers = metadata.controllers.iter().map(path_to_string);
            let exports = metadata.exports.iter().map(path_to_string);
            let attrs = &item.attrs;
            let visibility = &item.visibility;
            let generics = &item.generics;
            let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();

            quote! {
                #(#attrs)*
                #visibility struct #name #generics;

                impl #impl_generics ::nidus::prelude::Module for #name #type_generics #where_clause {
                    fn definition() -> ::nidus::prelude::ModuleDefinition {
                        ::nidus::prelude::ModuleBuilder::new(#module_name)
                            #(.import(#imports))*
                            #(.provider(#providers))*
                            #(.controller(#controllers))*
                            #(.export(#exports))*
                            .build()
                    }
                }
            }
        }
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[module] can only be used on module marker structs: {error}"),
            item,
        ),
    }
}

struct ModuleItem {
    attrs: Vec<Attribute>,
    visibility: Visibility,
    ident: Ident,
    generics: Generics,
    metadata: ModuleMetadata,
}

impl Parse for ModuleItem {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let visibility = input.parse()?;
        input.parse::<Token![struct]>()?;
        let ident = input.parse()?;
        let mut generics: Generics = input.parse()?;
        generics.where_clause = input.parse()?;

        let mut metadata = ModuleMetadata::default();
        if input.peek(Token![;]) {
            input.parse::<Token![;]>()?;
        } else {
            let content;
            braced!(content in input);
            while !content.is_empty() {
                let field: ModuleField = content.parse()?;
                metadata.extend_section(field.section, field.values)?;
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            attrs,
            visibility,
            ident,
            generics,
            metadata,
        })
    }
}

struct ModuleField {
    section: Ident,
    values: Vec<Path>,
}

impl Parse for ModuleField {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let section = input.parse()?;
        input.parse::<Token![:]>()?;
        let values = if input.peek(syn::token::Bracket) {
            let content;
            bracketed!(content in input);
            content
                .parse_terminated(Path::parse_mod_style, Token![,])?
                .into_iter()
                .collect::<Vec<_>>()
        } else if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            content
                .parse_terminated(Path::parse_mod_style, Token![,])?
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            return Err(input.error(
                "module metadata fields must use single-item [Path] syntax or tuple syntax like (First, Second)",
            ));
        };
        Ok(Self { section, values })
    }
}

#[derive(Default)]
struct ModuleMetadata {
    imports: Vec<Path>,
    providers: Vec<Path>,
    controllers: Vec<Path>,
    exports: Vec<Path>,
}

impl ModuleMetadata {
    fn extend(&mut self, other: ModuleMetadata) {
        self.imports.extend(other.imports);
        self.providers.extend(other.providers);
        self.controllers.extend(other.controllers);
        self.exports.extend(other.exports);
    }

    fn extend_section(&mut self, section: Ident, values: Vec<Path>) -> syn::Result<()> {
        match section.to_string().as_str() {
            "imports" => self.imports.extend(values),
            "providers" => self.providers.extend(values),
            "controllers" => self.controllers.extend(values),
            "exports" => self.exports.extend(values),
            other => {
                return Err(syn::Error::new(
                    section.span(),
                    format!("unknown module metadata section `{other}`"),
                ));
            }
        }
        Ok(())
    }
}

impl Parse for ModuleMetadata {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut metadata = ModuleMetadata::default();

        while !input.is_empty() {
            let section: Ident = input.parse()?;
            let content;
            parenthesized!(content in input);
            let values = content
                .parse_terminated(Path::parse_mod_style, Token![,])?
                .into_iter()
                .collect::<Vec<_>>();

            metadata.extend_section(section, values)?;

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        Ok(metadata)
    }
}

fn path_to_string(path: &Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.ident.to_token_stream().to_string())
        .unwrap_or_else(|| path.to_token_stream().to_string())
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::expand;

    #[test]
    fn snapshots_module_expansion_with_attribute_and_field_metadata() {
        let expanded = expand(
            quote! {
                imports(crate::database::DatabaseModule),
                providers(crate::users::UsersService)
            },
            quote! {
                pub struct UsersModule {
                    providers: (crate::users::UsersRepository,),
                    controllers: [crate::users::UsersController],
                    exports: [crate::users::UsersService],
                }
            },
        );

        insta::assert_snapshot!(expanded.to_string());
    }
}

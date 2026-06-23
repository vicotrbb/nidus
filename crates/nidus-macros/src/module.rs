use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Ident, ItemStruct, Path, Token, parenthesized, parse::Parse, parse::ParseStream, parse2,
};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let metadata = match parse2::<ModuleMetadata>(attr) {
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

    match parse2::<ItemStruct>(item.clone()) {
        Ok(item) => {
            let name = &item.ident;
            let module_name = name.to_string();
            let imports = metadata.imports.iter().map(path_to_string);
            let providers = metadata.providers.iter().map(path_to_string);
            let controllers = metadata.controllers.iter().map(path_to_string);
            let exports = metadata.exports.iter().map(path_to_string);

            quote! {
                #item

                impl ::nidus::prelude::Module for #name {
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
            format!("#[module] can only be used on structs: {error}"),
            item,
        ),
    }
}

#[derive(Default)]
struct ModuleMetadata {
    imports: Vec<Path>,
    providers: Vec<Path>,
    controllers: Vec<Path>,
    exports: Vec<Path>,
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

            match section.to_string().as_str() {
                "imports" => metadata.imports.extend(values),
                "providers" => metadata.providers.extend(values),
                "controllers" => metadata.controllers.extend(values),
                "exports" => metadata.exports.extend(values),
                other => {
                    return Err(syn::Error::new(
                        section.span(),
                        format!("unknown module metadata section `{other}`"),
                    ));
                }
            }

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

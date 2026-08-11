use syn::{
    Attribute, Field, Fields, Ident, Item, Meta, Path as SynPath, Token, Type, parenthesized,
    parse::{Parse, ParseStream},
    parse2,
};

#[derive(Debug, Default)]
pub(crate) struct DiscoveredModule {
    pub(crate) name: String,
    pub(crate) imports: Vec<String>,
    pub(crate) providers: Vec<String>,
    pub(crate) controllers: Vec<String>,
    pub(crate) exports: Vec<String>,
}

pub(crate) fn discover_module_macro_metadata(
    file: &syn::File,
) -> syn::Result<Vec<DiscoveredModule>> {
    let mut modules = Vec::new();
    for item in &file.items {
        let Item::Struct(item) = item else {
            continue;
        };
        let Some(mut module) = module_attr_metadata(&item.attrs)? else {
            continue;
        };
        module.name = item.ident.to_string();
        apply_module_field_metadata(&mut module, &item.fields);
        modules.push(module);
    }
    Ok(modules)
}

fn module_attr_metadata(attrs: &[Attribute]) -> syn::Result<Option<DiscoveredModule>> {
    let Some(attr) = attrs.iter().find(|attr| attr.path().is_ident("module")) else {
        return Ok(None);
    };
    let metadata = match &attr.meta {
        Meta::Path(_) => return Ok(Some(DiscoveredModule::default())),
        Meta::List(list) => parse2::<ModuleAttributeMetadata>(list.tokens.clone())?,
        Meta::NameValue(_) => {
            return Err(syn::Error::new_spanned(
                attr,
                "#[module] expects groups like providers(UsersService)",
            ));
        }
    };
    Ok(Some(metadata.into_discovered_module()))
}

fn apply_module_field_metadata(module: &mut DiscoveredModule, fields: &Fields) {
    let Fields::Named(fields) = fields else {
        return;
    };
    for field in &fields.named {
        let Some(name) = field.ident.as_ref().map(ToString::to_string) else {
            continue;
        };
        let values = type_values(field);
        match name.as_str() {
            "imports" => module.imports.extend(values),
            "providers" => module.providers.extend(values),
            "controllers" => module.controllers.extend(values),
            "exports" => module.exports.extend(values),
            _ => {}
        }
    }
}

fn type_values(field: &Field) -> Vec<String> {
    type_paths(&field.ty)
        .into_iter()
        .filter_map(path_name)
        .collect()
}

fn type_paths(ty: &Type) -> Vec<&syn::Path> {
    match ty {
        Type::Array(array) => type_paths(&array.elem),
        Type::Group(group) => type_paths(&group.elem),
        Type::Paren(paren) => type_paths(&paren.elem),
        Type::Path(path) => vec![&path.path],
        Type::Slice(slice) => type_paths(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().flat_map(type_paths).collect(),
        _ => Vec::new(),
    }
}

#[derive(Default)]
struct ModuleAttributeMetadata {
    imports: Vec<String>,
    providers: Vec<String>,
    controllers: Vec<String>,
    exports: Vec<String>,
}

impl ModuleAttributeMetadata {
    fn extend_section(&mut self, section: &Ident, values: Vec<String>) -> syn::Result<()> {
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

    fn into_discovered_module(self) -> DiscoveredModule {
        DiscoveredModule {
            imports: self.imports,
            providers: self.providers,
            controllers: self.controllers,
            exports: self.exports,
            ..DiscoveredModule::default()
        }
    }
}

impl Parse for ModuleAttributeMetadata {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut metadata = ModuleAttributeMetadata::default();

        while !input.is_empty() {
            let section: Ident = input.parse()?;
            let content;
            parenthesized!(content in input);
            let values = content
                .parse_terminated(SynPath::parse_mod_style, Token![,])?
                .into_iter()
                .filter_map(|path| path_name(&path))
                .collect();
            metadata.extend_section(&section, values)?;

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        Ok(metadata)
    }
}

fn path_name(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

#[cfg(test)]
mod tests {
    use super::discover_module_macro_metadata;

    #[test]
    fn discovers_module_attribute_and_field_metadata() {
        let file = syn::parse_file(
            r#"
use nidus::prelude::*;

#[module(
    imports(crate::database::DatabaseModule),
    providers(crate::users::UsersService)
)]
pub struct UsersModule {
    providers: (crate::users::UsersRepository,),
    controllers: [crate::users::UsersController],
    exports: [crate::users::UsersService],
}
"#,
        )
        .unwrap();

        let modules = discover_module_macro_metadata(&file).unwrap();

        assert_eq!(modules.len(), 1);
        let module = &modules[0];
        assert_eq!(module.name, "UsersModule");
        assert_eq!(module.imports, ["DatabaseModule"]);
        assert_eq!(module.providers, ["UsersService", "UsersRepository"]);
        assert_eq!(module.controllers, ["UsersController"]);
        assert_eq!(module.exports, ["UsersService"]);
    }
}

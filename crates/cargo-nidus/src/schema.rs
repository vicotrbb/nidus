use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use syn::{Fields, GenericArgument, Item, PathArguments, Type};

pub(crate) fn discover_schemas(
    root: &Path,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>> {
    let mut schemas = names
        .iter()
        .map(|name| (name.clone(), fallback_schema()))
        .collect::<BTreeMap<_, _>>();
    if names.is_empty() {
        return Ok(schemas);
    }

    for path in rust_source_files(&root.join("src"))? {
        let source =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let file =
            syn::parse_file(&source).with_context(|| format!("parsing {}", path.display()))?;
        for item in file.items {
            let Item::Struct(item) = item else {
                continue;
            };
            let name = item.ident.to_string();
            if names.contains(&name) {
                schemas.insert(name, schema_for_struct(&item.fields));
            }
        }
    }

    Ok(schemas)
}

fn rust_source_files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    collect_rust_source_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rust_source_files(path: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust_source_files(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn schema_for_struct(fields: &Fields) -> Value {
    let Fields::Named(fields) = fields else {
        return fallback_schema();
    };

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for field in &fields.named {
        let Some(identifier) = &field.ident else {
            continue;
        };
        let name = identifier.to_string();
        if !is_option_type(&field.ty) {
            required.push(name.clone());
        }
        properties.insert(name, schema_for_type(&field.ty));
    }

    let mut schema = json!({
        "type": "object",
        "properties": properties,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema
}

fn schema_for_type(ty: &Type) -> Value {
    if let Some(inner) = generic_inner_type(ty, "Option") {
        return schema_for_type(inner);
    }
    if let Some(inner) = generic_inner_type(ty, "Vec") {
        return json!({
            "type": "array",
            "items": schema_for_type(inner),
        });
    }
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
    {
        let name = segment.ident.to_string();
        return match name.as_str() {
            "String" | "str" => json!({ "type": "string" }),
            "bool" => json!({ "type": "boolean" }),
            "f32" | "f64" => json!({ "type": "number" }),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
            | "u128" | "usize" => json!({ "type": "integer" }),
            _ => json!({ "$ref": format!("#/components/schemas/{name}") }),
        };
    }
    fallback_schema()
}

fn generic_inner_type<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(inner) = arguments.args.first()? else {
        return None;
    };
    Some(inner)
}

fn is_option_type(ty: &Type) -> bool {
    generic_inner_type(ty, "Option").is_some()
}

fn fallback_schema() -> Value {
    json!({
        "type": "object"
    })
}

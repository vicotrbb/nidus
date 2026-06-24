use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use syn::{Field, Fields, GenericArgument, Item, LitStr, PathArguments, Type};

pub(crate) fn discover_schemas(
    root: &Path,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, Value>> {
    let mut schemas = BTreeMap::new();
    if names.is_empty() {
        return Ok(schemas);
    }

    let mut local = BTreeMap::new();
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
            local.insert(name, schema_for_struct(&item.fields));
        }
    }

    let mut pending = names.iter().cloned().collect::<BTreeSet<_>>();
    while let Some(name) = pending.pop_first() {
        if schemas.contains_key(&name) {
            continue;
        }
        let Some(schema) = local.get(&name) else {
            schemas.insert(name, fallback_schema());
            continue;
        };

        schemas.insert(name, schema.value.clone());
        for reference in &schema.references {
            if !schemas.contains_key(reference) {
                pending.insert(reference.clone());
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

#[derive(Clone)]
struct InferredSchema {
    value: Value,
    references: BTreeSet<String>,
}

impl InferredSchema {
    fn new(value: Value) -> Self {
        Self {
            value,
            references: BTreeSet::new(),
        }
    }
}

fn schema_for_struct(fields: &Fields) -> InferredSchema {
    let Fields::Named(fields) = fields else {
        return InferredSchema::new(fallback_schema());
    };

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    let mut references = BTreeSet::new();

    for field in &fields.named {
        if has_serde_flag(field, "skip") {
            continue;
        }
        let Some(name) = field_name(field) else {
            continue;
        };
        if field_is_required(field) {
            required.push(name.clone());
        }
        let schema = schema_for_type(&field.ty);
        references.extend(schema.references);
        properties.insert(name, schema.value);
    }

    let mut schema = json!({
        "type": "object",
        "properties": properties,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    InferredSchema {
        value: schema,
        references,
    }
}

fn schema_for_type(ty: &Type) -> InferredSchema {
    if let Some(inner) = generic_inner_type(ty, "Option") {
        return schema_for_type(inner);
    }
    if let Some(inner) = generic_inner_type(ty, "Vec") {
        let item_schema = schema_for_type(inner);
        return InferredSchema {
            value: json!({
                "type": "array",
                "items": item_schema.value,
            }),
            references: item_schema.references,
        };
    }
    if let Type::Path(path) = ty
        && let Some(segment) = path.path.segments.last()
    {
        let name = segment.ident.to_string();
        return match primitive_schema(&name) {
            Some(schema) => InferredSchema::new(schema),
            None => {
                let mut references = BTreeSet::new();
                references.insert(name.clone());
                InferredSchema {
                    value: json!({ "$ref": format!("#/components/schemas/{name}") }),
                    references,
                }
            }
        };
    }
    InferredSchema::new(fallback_schema())
}

fn primitive_schema(name: &str) -> Option<Value> {
    let schema = match name {
        "String" | "str" => json!({ "type": "string" }),
        "bool" => json!({ "type": "boolean" }),
        "f32" | "f64" => json!({ "type": "number" }),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => json!({ "type": "integer" }),
        _ => return None,
    };
    Some(schema)
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

fn field_is_required(field: &Field) -> bool {
    !is_option_type(&field.ty) && !has_serde_flag(field, "default")
}

fn field_name(field: &Field) -> Option<String> {
    let mut rename = None;
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value = meta.value()?;
                let literal: LitStr = value.parse()?;
                rename = Some(literal.value());
            }
            Ok(())
        });
    }

    rename.or_else(|| field.ident.as_ref().map(ToString::to_string))
}

fn has_serde_flag(field: &Field, name: &str) -> bool {
    let mut found = false;
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(name) {
                found = true;
            }
            Ok(())
        });
    }
    found
}

fn fallback_schema() -> Value {
    json!({
        "type": "object"
    })
}

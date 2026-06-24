use std::{fs, path::Path};

use anyhow::{Context, Result};
use syn::{Attribute, Expr, Field, Fields, Item, Lit, Meta, Stmt, Type};

pub(crate) fn inspect_graph(root: &Path) -> Result<()> {
    for module in discover_modules(root)? {
        println!("{}", module.name);
        if !module.imports.is_empty() {
            println!("  imports: {}", module.imports.join(", "));
        }
        if !module.providers.is_empty() {
            println!("  providers: {}", module.providers.join(", "));
        }
        if !module.controllers.is_empty() {
            println!("  controllers: {}", module.controllers.join(", "));
        }
        if !module.exports.is_empty() {
            println!("  exports: {}", module.exports.join(", "));
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct DiscoveredModule {
    name: String,
    imports: Vec<String>,
    providers: Vec<String>,
    controllers: Vec<String>,
    exports: Vec<String>,
}

fn discover_modules(root: &Path) -> Result<Vec<DiscoveredModule>> {
    let mut sources = Vec::new();
    for root_source in ["main.rs", "lib.rs"] {
        let path = root.join("src").join(root_source);
        if path.exists() {
            sources.push(path);
        }
    }
    let modules = root.join("src/modules");
    if modules.exists() {
        for entry in
            fs::read_dir(&modules).with_context(|| format!("reading {}", modules.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                sources.push(path);
            }
        }
    }
    sources.sort();

    let mut discovered = Vec::new();
    for path in sources {
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let file =
            syn::parse_file(&contents).with_context(|| format!("parsing {}", path.display()))?;
        let modules = discover_modules_in_source(&file);
        if modules.is_empty() {
            discovered.extend(extract_struct_names(&file).into_iter().map(|name| {
                DiscoveredModule {
                    name,
                    ..DiscoveredModule::default()
                }
            }));
        } else {
            discovered.extend(modules);
        }
    }
    discovered.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(discovered)
}

fn discover_modules_in_source(file: &syn::File) -> Vec<DiscoveredModule> {
    let mut modules = Vec::new();
    modules.extend(discover_module_builder_metadata(file));
    modules.extend(discover_module_macro_metadata(file));
    modules
}

fn discover_module_builder_metadata(file: &syn::File) -> Vec<DiscoveredModule> {
    let mut modules = Vec::new();
    for item in &file.items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        for item in &implementation.items {
            let syn::ImplItem::Fn(function) = item else {
                continue;
            };
            for statement in &function.block.stmts {
                if let Some(module) = module_from_statement(statement) {
                    modules.push(module);
                }
            }
        }
    }
    modules
}

fn module_from_statement(statement: &Stmt) -> Option<DiscoveredModule> {
    match statement {
        Stmt::Expr(expr, _) => module_from_expr(expr),
        _ => None,
    }
}

fn module_from_expr(expr: &Expr) -> Option<DiscoveredModule> {
    let Expr::MethodCall(call) = expr else {
        return None;
    };
    (call.method == "build")
        .then(|| module_from_builder_chain(&call.receiver))
        .flatten()
}

fn module_from_builder_chain(expr: &Expr) -> Option<DiscoveredModule> {
    match expr {
        Expr::Call(call) => {
            let Expr::Path(path) = &*call.func else {
                return None;
            };
            let is_module_builder_new = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .ends_with(&["ModuleBuilder".to_owned(), "new".to_owned()]);
            if !is_module_builder_new {
                return None;
            }
            call.args
                .first()
                .and_then(string_literal)
                .map(|name| DiscoveredModule {
                    name,
                    ..DiscoveredModule::default()
                })
        }
        Expr::MethodCall(call) => {
            let mut module = module_from_builder_chain(&call.receiver)?;
            let Some(value) = call.args.first().and_then(string_literal) else {
                return Some(module);
            };
            match call.method.to_string().as_str() {
                "import" => module.imports.push(value),
                "provider" => module.providers.push(value),
                "controller" => module.controllers.push(value),
                "export" => module.exports.push(value),
                _ => {}
            }
            Some(module)
        }
        _ => None,
    }
}

fn discover_module_macro_metadata(file: &syn::File) -> Vec<DiscoveredModule> {
    let mut modules = Vec::new();
    for item in &file.items {
        let Item::Struct(item) = item else {
            continue;
        };
        let Some(mut module) = module_attr_metadata(&item.attrs) else {
            continue;
        };
        module.name = item.ident.to_string();
        apply_module_field_metadata(&mut module, &item.fields);
        modules.push(module);
    }
    modules
}

fn module_attr_metadata(attrs: &[Attribute]) -> Option<DiscoveredModule> {
    let attr = attrs.iter().find(|attr| attr.path().is_ident("module"))?;
    let args = match &attr.meta {
        Meta::Path(_) => return Some(DiscoveredModule::default()),
        Meta::List(list) => list.tokens.to_string(),
        Meta::NameValue(_) => return Some(DiscoveredModule::default()),
    };
    Some(DiscoveredModule {
        imports: extract_module_attr_values(&args, "imports").unwrap_or_default(),
        providers: extract_module_attr_values(&args, "providers").unwrap_or_default(),
        controllers: extract_module_attr_values(&args, "controllers").unwrap_or_default(),
        exports: extract_module_attr_values(&args, "exports").unwrap_or_default(),
        ..DiscoveredModule::default()
    })
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

fn extract_struct_names(file: &syn::File) -> Vec<String> {
    file.items
        .iter()
        .filter_map(|item| {
            let Item::Struct(item) = item else {
                return None;
            };
            matches!(item.vis, syn::Visibility::Public(_)).then(|| item.ident.to_string())
        })
        .collect()
}

fn extract_module_attr_values(args: &str, field: &str) -> Option<Vec<String>> {
    let start = args.find(field)? + field.len();
    extract_group_values(&args[start..])
}

fn extract_group_values(rest: &str) -> Option<Vec<String>> {
    let rest = rest.trim_start();
    let (open, close) = match rest.chars().next()? {
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        _ => return None,
    };
    let values = rest.strip_prefix(open)?;
    let end = values.find(close)?;
    let values = &values[..end];
    Some(
        values
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(path_last_segment)
            .collect(),
    )
}

fn path_last_segment(path: &str) -> String {
    path.split("::")
        .last()
        .map(str::trim)
        .unwrap_or(path)
        .to_owned()
}

fn path_name(path: &syn::Path) -> Option<String> {
    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn string_literal(expr: &Expr) -> Option<String> {
    let Expr::Lit(lit) = expr else {
        return None;
    };
    let Lit::Str(value) = &lit.lit else {
        return None;
    };
    Some(value.value())
}

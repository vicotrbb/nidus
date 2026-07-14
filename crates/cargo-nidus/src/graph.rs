use std::{fs, path::Path};

use anyhow::{Context, Result};
use syn::{Expr, Item, Lit, Stmt};

use crate::graph_metadata::{
    DiscoveredModule, discover_module_macro_metadata, extract_struct_names,
};
use crate::source_files::rust_source_files;

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

fn discover_modules(root: &Path) -> Result<Vec<DiscoveredModule>> {
    let sources = rust_source_files(&root.join("src"))?;

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

fn string_literal(expr: &Expr) -> Option<String> {
    let Expr::Lit(lit) = expr else {
        return None;
    };
    let Lit::Str(value) = &lit.lit else {
        return None;
    };
    Some(value.value())
}

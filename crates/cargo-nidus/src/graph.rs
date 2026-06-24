use std::{fs, path::Path};

use anyhow::{Context, Result};

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

    let mut discovered = Vec::new();
    for path in sources {
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let modules = discover_modules_in_source(&contents);
        if modules.is_empty() {
            discovered.extend(extract_struct_names(&contents).into_iter().map(|name| {
                DiscoveredModule {
                    name,
                    ..DiscoveredModule::default()
                }
            }));
        } else {
            discovered.extend(modules);
        }
    }
    Ok(discovered)
}

fn discover_modules_in_source(contents: &str) -> Vec<DiscoveredModule> {
    let mut modules = Vec::new();
    let mut current = None::<DiscoveredModule>;

    for line in contents.lines() {
        if let Some(name) = extract_call_arg(line, "ModuleBuilder::new") {
            if let Some(module) = current.take() {
                modules.push(module);
            }
            current = Some(DiscoveredModule {
                name,
                ..DiscoveredModule::default()
            });
        }

        if let Some(module) = current.as_mut() {
            if let Some(import) = extract_call_arg(line, ".import") {
                module.imports.push(import);
            }
            if let Some(provider) = extract_call_arg(line, ".provider") {
                module.providers.push(provider);
            }
            if let Some(controller) = extract_call_arg(line, ".controller") {
                module.controllers.push(controller);
            }
            if let Some(export) = extract_call_arg(line, ".export") {
                module.exports.push(export);
            }
        }

        if line.contains(".build()")
            && let Some(module) = current.take()
        {
            modules.push(module);
        }
    }

    if let Some(module) = current {
        modules.push(module);
    }
    modules.extend(discover_module_macro_metadata(contents));
    modules
}

fn discover_module_macro_metadata(contents: &str) -> Vec<DiscoveredModule> {
    let mut modules = Vec::new();
    let mut module_attr = None::<String>;
    let mut pending_module_attr = None::<DiscoveredModule>;
    let mut current = None::<DiscoveredModule>;

    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(attr) = module_attr.as_mut() {
            attr.push(' ');
            attr.push_str(trimmed);
            if trimmed.ends_with(']') {
                pending_module_attr = Some(discover_module_attr_metadata(attr));
                module_attr = None;
            }
            continue;
        }

        if trimmed.starts_with("#[module") {
            if trimmed.ends_with(']') {
                pending_module_attr = Some(discover_module_attr_metadata(trimmed));
            } else {
                module_attr = Some(trimmed.to_owned());
            }
            continue;
        }

        if pending_module_attr.is_some()
            && let Some(name) = extract_module_struct_name(trimmed)
        {
            let has_body = trimmed.contains('{');
            let is_unit = trimmed.contains(';');
            let mut module = pending_module_attr.take().unwrap_or_default();
            module.name = name;
            current = Some(module);

            if is_unit {
                if let Some(module) = current.take() {
                    modules.push(module);
                }
            } else if !has_body {
                current = None;
            }
            continue;
        }

        if let Some(module) = current.as_mut() {
            if let Some(values) = extract_module_field_values(trimmed, "imports") {
                module.imports.extend(values);
            }
            if let Some(values) = extract_module_field_values(trimmed, "providers") {
                module.providers.extend(values);
            }
            if let Some(values) = extract_module_field_values(trimmed, "controllers") {
                module.controllers.extend(values);
            }
            if let Some(values) = extract_module_field_values(trimmed, "exports") {
                module.exports.extend(values);
            }
        }

        if trimmed.starts_with('}')
            && let Some(module) = current.take()
        {
            modules.push(module);
        }
    }

    modules
}

fn discover_module_attr_metadata(line: &str) -> DiscoveredModule {
    let Some(args) = extract_module_attr_args(line) else {
        return DiscoveredModule::default();
    };

    DiscoveredModule {
        imports: extract_module_attr_values(args, "imports").unwrap_or_default(),
        providers: extract_module_attr_values(args, "providers").unwrap_or_default(),
        controllers: extract_module_attr_values(args, "controllers").unwrap_or_default(),
        exports: extract_module_attr_values(args, "exports").unwrap_or_default(),
        ..DiscoveredModule::default()
    }
}

fn extract_call_arg(line: &str, call: &str) -> Option<String> {
    let start = line.find(call)? + call.len();
    let rest = line[start..].trim_start();
    let rest = rest.strip_prefix("(\"")?;
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn extract_struct_names(contents: &str) -> Vec<String> {
    contents
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub struct "))
        .filter_map(|rest| rest.split([';', '{', '(']).next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn extract_module_struct_name(line: &str) -> Option<String> {
    let start = line.find("struct ")? + "struct ".len();
    let rest = line[start..].trim_start();
    let name = rest
        .split([' ', '{', ';', '(', '<'])
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())?;
    Some(name.to_owned())
}

fn extract_module_attr_args(line: &str) -> Option<&str> {
    line.strip_prefix("#[module(")?.strip_suffix(")]")
}

fn extract_module_attr_values(args: &str, field: &str) -> Option<Vec<String>> {
    let start = args.find(field)? + field.len();
    extract_group_values(&args[start..])
}

fn extract_module_field_values(line: &str, field: &str) -> Option<Vec<String>> {
    let start = line.find(field)? + field.len();
    let rest = line[start..].trim_start().strip_prefix(':')?.trim_start();
    extract_group_values(rest)
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

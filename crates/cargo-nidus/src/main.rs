//! Command-line tooling for generating and inspecting Nidus projects.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};

mod check;
mod generate;
mod routes;
mod source_openapi;

use check::check_project;
use generate::{create_project, generate_artifact};
use routes::{discover_routes, inspect_routes, openapi_path_parameters};

#[derive(Debug, Parser)]
#[command(name = "cargo-nidus", bin_name = "cargo nidus")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new Nidus application.
    New {
        /// Project name.
        name: String,
        /// Directory where the project folder should be created.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Local path to the nidus facade crate, used by tests and unreleased development builds.
        #[arg(long, hide = true)]
        nidus_path: Option<PathBuf>,
    },
    /// Generate a framework artifact.
    Generate {
        /// Artifact kind: module, controller, service, or repository.
        kind: String,
        /// Artifact name.
        name: String,
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print route metadata.
    Routes {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print dependency graph metadata.
    Graph {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print expanded generated code guidance.
    Expand {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Print the cargo-expand command without running it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Check project structure.
    Check {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Generate OpenAPI output.
    Openapi {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let args = if args.get(1).is_some_and(|arg| arg == "nidus") {
        let mut stripped = Vec::with_capacity(args.len() - 1);
        stripped.push(args[0].clone());
        stripped.extend(args.iter().skip(2).cloned());
        stripped
    } else {
        args
    };
    let cli = Cli::parse_from(args);

    match cli.command {
        Command::New {
            name,
            path,
            nidus_path,
        } => create_project(&name, &path, nidus_path.as_deref()),
        Command::Generate { kind, name, path } => generate_artifact(&kind, &name, &path),
        Command::Routes { path } => inspect_routes(&path),
        Command::Graph { path } => inspect_graph(&path),
        Command::Expand { path, dry_run } => expand_project(&path, dry_run),
        Command::Check { path } => check_project(&path),
        Command::Openapi { path } => generate_openapi(&path),
    }
}

fn inspect_graph(root: &Path) -> Result<()> {
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

fn expand_project(root: &Path, dry_run: bool) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if !manifest.exists() {
        bail!(
            "Nidus expand failed for {}. Missing required file: Cargo.toml",
            root.display()
        );
    }

    if dry_run {
        println!("cargo expand --manifest-path {}", manifest.display());
        return Ok(());
    }

    let status = ProcessCommand::new("cargo")
        .arg("expand")
        .arg("--manifest-path")
        .arg(&manifest)
        .status()
        .with_context(|| "running cargo expand")?;
    if !status.success() {
        bail!("cargo expand failed for {}", root.display());
    }
    Ok(())
}

fn generate_openapi(root: &Path) -> Result<()> {
    let mut paths = serde_json::Map::new();
    let mut schema_names = BTreeSet::new();
    for route in discover_routes(root)? {
        let parameters = openapi_path_parameters(&route.path);
        let response_status = route.response_status.unwrap_or(200).to_string();
        let entry = paths
            .entry(route.path)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(methods) = entry {
            let mut operation = serde_json::Map::from_iter([(
                "responses".to_owned(),
                json!({
                    response_status.clone(): {
                        "description": "Success"
                    }
                }),
            )]);
            if let Some(summary) = route.summary {
                operation.insert("summary".to_owned(), json!(summary));
            }
            if !route.tags.is_empty() {
                operation.insert("tags".to_owned(), json!(route.tags));
            }
            if let Some(schema) = route.request_schema {
                schema_names.insert(schema.clone());
                operation.insert(
                    "requestBody".to_owned(),
                    json!({
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref(&schema)
                            }
                        }
                    }),
                );
            }
            if let Some(schema) = route.response_schema {
                schema_names.insert(schema.clone());
                operation.insert(
                    "responses".to_owned(),
                    json!({
                        response_status: {
                            "description": "Success",
                            "content": {
                                "application/json": {
                                    "schema": schema_ref(&schema)
                                }
                            }
                        }
                    }),
                );
            }
            if !parameters.is_empty() {
                operation.insert(
                    "parameters".to_owned(),
                    json!(
                        parameters
                            .into_iter()
                            .map(|name| {
                                json!({
                                    "name": name,
                                    "in": "path",
                                    "required": true,
                                    "schema": {
                                        "type": "string"
                                    }
                                })
                            })
                            .collect::<Vec<_>>()
                    ),
                );
            }
            methods.insert(route.method, Value::Object(operation));
        }
    }

    let mut document = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Nidus API",
            "version": "0.1.0",
        },
        "paths": paths,
    });

    if !schema_names.is_empty() {
        let schemas = schema_names
            .into_iter()
            .map(|name| {
                (
                    name,
                    json!({
                        "type": "object"
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        document["components"] = json!({
            "schemas": schemas,
        });
    }

    println!("{}", document);
    Ok(())
}

fn schema_ref(schema: &str) -> Value {
    json!({
        "$ref": format!("#/components/schemas/{schema}")
    })
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

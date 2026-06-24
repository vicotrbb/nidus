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

use check::check_project;
use generate::{create_project, generate_artifact};

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

fn inspect_routes(root: &Path) -> Result<()> {
    for route in discover_routes(root)? {
        let method = route.method.to_uppercase();
        let path = route.path;
        let mut line = format!("{method} {path}");
        if let Some(summary) = route.summary {
            line.push_str(&format!(" - {summary}"));
        }
        let mut annotations = Vec::new();
        if !route.guards.is_empty() {
            annotations.push(format!("guards: {}", route.guards.join(", ")));
        }
        if !route.pipes.is_empty() {
            annotations.push(format!("pipes: {}", route.pipes.join(", ")));
        }
        if route.validates {
            annotations.push("validates".to_owned());
        }
        if !annotations.is_empty() {
            line.push_str(&format!(" [{}]", annotations.join("; ")));
        }
        println!("{line}");
    }
    Ok(())
}

#[derive(Debug)]
struct DiscoveredRoute {
    method: String,
    path: String,
    summary: Option<String>,
    tags: Vec<String>,
    response_status: Option<u16>,
    request_schema: Option<String>,
    response_schema: Option<String>,
    guards: Vec<String>,
    pipes: Vec<String>,
    validates: bool,
}

fn discover_routes(root: &Path) -> Result<Vec<DiscoveredRoute>> {
    let controllers = root.join("src/controllers");
    if !controllers.exists() {
        return Ok(Vec::new());
    }

    let mut routes = Vec::new();
    for entry in
        fs::read_dir(&controllers).with_context(|| format!("reading {}", controllers.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let Some(prefix) = extract_attr_value(&contents, "controller") else {
            continue;
        };
        routes.extend(discover_controller_routes(&prefix, &contents)?);
    }
    Ok(routes)
}

fn discover_controller_routes(prefix: &str, contents: &str) -> Result<Vec<DiscoveredRoute>> {
    let mut routes = Vec::new();
    let mut openapi_attr = None::<String>;
    let mut pending_route = None;
    let mut pending_summary = None;
    let mut pending_tags = Vec::new();
    let mut pending_response_status = None;
    let mut pending_request_schema = None;
    let mut pending_response_schema = None;
    let mut pending_guards = Vec::new();
    let mut pending_pipes = Vec::new();
    let mut pending_validates = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(attr) = openapi_attr.as_mut() {
            attr.push(' ');
            attr.push_str(trimmed);
            if trimmed.ends_with(']') {
                let args = extract_openapi_args_from_line(attr)
                    .context("parsing multiline #[openapi] metadata")?;
                apply_openapi_args(
                    &args,
                    &mut pending_summary,
                    &mut pending_tags,
                    &mut pending_response_status,
                    &mut pending_request_schema,
                    &mut pending_response_schema,
                )?;
                openapi_attr = None;
            }
            continue;
        }

        if let Some((method, path)) = extract_route_attr_from_line(line) {
            pending_route = Some((method, path));
        }
        if let Some(guard) = extract_type_attr_from_line(line, "guard") {
            pending_guards.push(guard);
        }
        if let Some(pipe) = extract_type_attr_from_line(line, "pipe") {
            pending_pipes.push(pipe);
        }
        if trimmed == "#[validate]" {
            pending_validates = true;
        }
        if trimmed.starts_with("#[openapi(") {
            if trimmed.ends_with(")]") {
                if let Some(args) = extract_openapi_args_from_line(trimmed) {
                    apply_openapi_args(
                        &args,
                        &mut pending_summary,
                        &mut pending_tags,
                        &mut pending_response_status,
                        &mut pending_request_schema,
                        &mut pending_response_schema,
                    )?;
                }
            } else {
                openapi_attr = Some(trimmed.to_owned());
            }
        }

        if line.contains("fn ") {
            if let Some((method, path)) = pending_route.take() {
                routes.push(DiscoveredRoute {
                    method,
                    path: join_route(prefix, &path)?,
                    summary: pending_summary.take(),
                    tags: std::mem::take(&mut pending_tags),
                    response_status: pending_response_status.take(),
                    request_schema: pending_request_schema.take(),
                    response_schema: pending_response_schema.take(),
                    guards: std::mem::take(&mut pending_guards),
                    pipes: std::mem::take(&mut pending_pipes),
                    validates: pending_validates,
                });
                pending_validates = false;
            } else {
                pending_summary = None;
                pending_tags.clear();
                pending_response_status = None;
                pending_request_schema = None;
                pending_response_schema = None;
                pending_guards.clear();
                pending_pipes.clear();
                pending_validates = false;
            }
        }
    }

    if openapi_attr.is_some() {
        bail!("unterminated #[openapi] metadata");
    }

    Ok(routes)
}

fn apply_openapi_args(
    args: &str,
    pending_summary: &mut Option<String>,
    pending_tags: &mut Vec<String>,
    pending_response_status: &mut Option<u16>,
    pending_request_schema: &mut Option<String>,
    pending_response_schema: &mut Option<String>,
) -> Result<()> {
    validate_openapi_args(args)?;
    let Some(summary) = extract_openapi_summary(args)? else {
        bail!("#[openapi] requires summary = \"...\" metadata");
    };
    *pending_summary = Some(summary);
    *pending_tags = extract_openapi_tags(args)?;
    *pending_response_status = extract_openapi_status(args)?;
    *pending_request_schema = extract_openapi_schema(args, "request")?;
    *pending_response_schema = extract_openapi_schema(args, "response")?;
    Ok(())
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

fn extract_attr_value(contents: &str, attr: &str) -> Option<String> {
    extract_all_attr_values(contents, attr).into_iter().next()
}

fn extract_all_attr_values(contents: &str, attr: &str) -> Vec<String> {
    contents
        .lines()
        .filter_map(|line| extract_attr_value_from_line(line, attr))
        .collect()
}

fn extract_attr_value_from_line(line: &str, attr: &str) -> Option<String> {
    let needle = format!("#[{attr}(\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn extract_route_attr_from_line(line: &str) -> Option<(String, String)> {
    for method in ["get", "post", "put", "patch", "delete"] {
        if let Some(path) = extract_attr_value_from_line(line, method) {
            return Some((method.to_owned(), path));
        }
    }
    None
}

fn extract_type_attr_from_line(line: &str, attr: &str) -> Option<String> {
    let needle = format!("#[{attr}(");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find(")]")?;
    let value = rest[..end].trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn extract_openapi_args_from_line(line: &str) -> Option<String> {
    let needle = "#[openapi(";
    let start = line.find(needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.rfind(")]")?;
    Some(rest[..end].to_owned())
}

fn extract_openapi_summary(args: &str) -> Result<Option<String>> {
    for arg in split_openapi_args(args) {
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key.trim() != "summary" {
            continue;
        }
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        else {
            bail!("#[openapi] summary must be a string literal");
        };
        return Ok(Some(value.to_owned()));
    }
    Ok(None)
}

fn validate_openapi_args(args: &str) -> Result<()> {
    for arg in split_openapi_args(args) {
        let Some((key, _value)) = arg.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !matches!(key, "summary" | "tags" | "status" | "request" | "response") {
            bail!(
                "#[openapi] supports only summary = \"...\", tags = [\"...\"], status = 201, request = Type, and response = Type metadata"
            );
        }
    }
    Ok(())
}

fn split_openapi_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut bracket_depth = 0usize;
    let mut in_string = false;
    let mut previous_was_escape = false;

    for (index, character) in args.char_indices() {
        if in_string {
            if character == '"' && !previous_was_escape {
                in_string = false;
            }
            previous_was_escape = character == '\\' && !previous_was_escape;
            continue;
        }

        previous_was_escape = false;
        match character {
            '"' => in_string = true,
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if bracket_depth == 0 => {
                parts.push(args[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(args[start..].trim());
    parts
}

fn extract_openapi_tags(args: &str) -> Result<Vec<String>> {
    for arg in split_openapi_args(args) {
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key.trim() != "tags" {
            continue;
        }
        let value = value.trim();
        let Some(tags) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        else {
            bail!("#[openapi] tags must be an array of string literals");
        };
        let mut values = Vec::new();
        for raw in tags.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let Some(value) = raw
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
            else {
                bail!("#[openapi] tags must be string literals");
            };
            values.push(value.to_owned());
        }
        return Ok(values);
    }
    Ok(Vec::new())
}

fn extract_openapi_status(args: &str) -> Result<Option<u16>> {
    for arg in split_openapi_args(args) {
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key.trim() != "status" {
            continue;
        }
        let value = value.trim();
        let status = value
            .parse::<u16>()
            .with_context(|| "#[openapi] status must be an HTTP status code integer literal")?;
        if !(100..=599).contains(&status) {
            bail!("#[openapi] status must be in the HTTP status code range 100..=599");
        }
        return Ok(Some(status));
    }
    Ok(None)
}

fn extract_openapi_schema(args: &str, key: &str) -> Result<Option<String>> {
    for arg in split_openapi_args(args) {
        let Some((name, value)) = arg.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        if value.starts_with('"') || value.is_empty() {
            bail!("#[openapi] {key} must be a type path");
        }
        if !is_type_path(value) {
            bail!("#[openapi] {key} must be a type path");
        }
        let schema = value
            .split("::")
            .last()
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| anyhow::anyhow!("#[openapi] {key} must be a type path"))?;
        return Ok(Some(schema.to_owned()));
    }
    Ok(None)
}

fn is_type_path(value: &str) -> bool {
    value.split("::").all(is_type_segment)
}

fn is_type_segment(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
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

fn join_route(prefix: &str, route: &str) -> Result<String> {
    let prefix = normalize_path(prefix)?;
    let route = normalize_path(route)?;
    let joined = if prefix == "/" {
        route
    } else if route == "/" {
        format!("{prefix}/")
    } else {
        format!("{prefix}{route}")
    };
    Ok(convert_nest_params(&joined))
}

fn normalize_path(path: &str) -> Result<String> {
    let path = path.trim();
    validate_path(path)?;
    let normalized = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    Ok(normalized)
}

fn validate_path(path: &str) -> Result<()> {
    for segment in path.split('/') {
        if segment == ":" {
            bail!("route path `{path}` contains a parameter segment without a name after ':'");
        }
    }
    Ok(())
}

fn convert_nest_params(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if let Some(name) = segment.strip_prefix(':') {
                format!("{{{name}}}")
            } else {
                segment.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn openapi_path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let name = segment.strip_prefix('{')?.strip_suffix('}')?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use syn::{Attribute, ImplItem, Item, ItemImpl, ItemStruct, LitStr, Meta, PathArguments, Type};

use crate::route_order::sort_discovered_routes;
use crate::route_path::join_route;
use crate::source_files::rust_source_files;
use crate::source_openapi::parse_openapi_args;

const ROUTE_METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

pub(crate) fn inspect_routes(root: &Path) -> Result<()> {
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
pub(crate) struct DiscoveredRoute {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) summary: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) response_status: Option<u16>,
    pub(crate) request_schema: Option<String>,
    pub(crate) response_schema: Option<String>,
    pub(crate) guards: Vec<String>,
    pub(crate) pipes: Vec<String>,
    pub(crate) validates: bool,
}

pub(crate) fn discover_routes(root: &Path) -> Result<Vec<DiscoveredRoute>> {
    let mut files = Vec::new();
    let mut global_prefixes = HashMap::<String, Option<String>>::new();

    // Parse every source file before discovering routes so a controller and its
    // `#[routes]` impl may live in different files. Short names that occur more
    // than once are deliberately excluded from the cross-file fallback; the
    // file-local controller remains authoritative in that case.
    for path in rust_source_files(&root.join("src"))? {
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let file = syn::parse_file(&contents).map_err(|error| {
            if has_unterminated_openapi_attr(&contents) {
                anyhow::anyhow!("unterminated #[openapi] metadata")
            } else {
                anyhow::Error::new(error).context(format!("parsing {}", path.display()))
            }
        })?;
        let local_prefixes = controller_prefixes(&file)?;
        for (name, prefix) in &local_prefixes {
            match global_prefixes.entry(name.clone()) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Some(prefix.clone()));
                }
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.insert(None);
                }
            }
        }
        files.push((file, local_prefixes));
    }

    let mut routes = Vec::new();
    for (file, local_prefixes) in &files {
        routes.extend(discover_controller_routes_with_prefixes(
            file,
            local_prefixes,
            &global_prefixes,
        )?);
    }
    sort_discovered_routes(&mut routes);
    reject_duplicate_routes(&routes)?;
    Ok(routes)
}

fn reject_duplicate_routes(routes: &[DiscoveredRoute]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for route in routes {
        if !seen.insert((route.method.as_str(), route.path.as_str())) {
            bail!(
                "duplicate route declaration for {} {}",
                route.method.to_uppercase(),
                route.path
            );
        }
    }
    Ok(())
}

#[cfg(test)]
fn discover_controller_routes(file: &syn::File) -> Result<Vec<DiscoveredRoute>> {
    let controller_prefixes = controller_prefixes(file)?;
    discover_controller_routes_with_prefixes(file, &controller_prefixes, &HashMap::new())
}

fn discover_controller_routes_with_prefixes(
    file: &syn::File,
    local_prefixes: &HashMap<String, String>,
    global_prefixes: &HashMap<String, Option<String>>,
) -> Result<Vec<DiscoveredRoute>> {
    let mut routes = Vec::new();

    for item in &file.items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        let Some(controller_name) = impl_self_type_name(implementation) else {
            continue;
        };
        let prefix = local_prefixes.get(&controller_name).or_else(|| {
            global_prefixes
                .get(&controller_name)
                .and_then(Option::as_ref)
        });
        let Some(prefix) = prefix else {
            if matches!(global_prefixes.get(&controller_name), Some(None))
                && implementation.items.iter().any(|item| {
                    let ImplItem::Fn(function) = item else {
                        return false;
                    };
                    ROUTE_METHODS
                        .iter()
                        .any(|method| route_method_attr(&function.attrs, method).is_some())
                })
            {
                bail!(
                    "ambiguous cross-file controller `{controller_name}`; keep its #[controller] definition in the same file as the #[routes] impl or use a unique controller type name"
                );
            }
            continue;
        };

        for item in &implementation.items {
            let ImplItem::Fn(function) = item else {
                continue;
            };
            let Some((method, route_path)) = route_attr(&function.attrs)? else {
                continue;
            };
            let openapi = openapi_attr(&function.attrs)?;
            routes.push(DiscoveredRoute {
                method,
                path: join_route(prefix, &route_path)?,
                summary: openapi.as_ref().map(|metadata| metadata.summary.clone()),
                tags: openapi
                    .as_ref()
                    .map(|metadata| metadata.tags.clone())
                    .unwrap_or_default(),
                response_status: openapi
                    .as_ref()
                    .and_then(|metadata| metadata.response_status),
                request_schema: openapi
                    .as_ref()
                    .and_then(|metadata| metadata.request_schema.clone()),
                response_schema: openapi
                    .as_ref()
                    .and_then(|metadata| metadata.response_schema.clone()),
                guards: type_attrs(&function.attrs, "guard")?,
                pipes: type_attrs(&function.attrs, "pipe")?,
                validates: has_attr(&function.attrs, "validate"),
            });
        }
    }

    Ok(routes)
}

fn controller_prefixes(file: &syn::File) -> Result<HashMap<String, String>> {
    let mut prefixes = HashMap::new();
    for item in &file.items {
        let Item::Struct(item) = item else {
            continue;
        };
        if let Some(prefix) = controller_prefix(item)? {
            prefixes.insert(item.ident.to_string(), prefix);
        }
    }
    Ok(prefixes)
}

fn controller_prefix(item: &ItemStruct) -> Result<Option<String>> {
    let attrs = item
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("controller"))
        .collect::<Vec<_>>();
    if attrs.is_empty() {
        return Ok(None);
    }
    if attrs.len() > 1 {
        bail!("controller structs can declare at most one #[controller] attribute");
    }

    let prefix = attrs[0]
        .parse_args::<LitStr>()
        .with_context(
            || "#[controller] requires a string literal path like #[controller(\"/users\")]",
        )?
        .value();
    join_route(&prefix, "/")?;
    Ok(Some(prefix))
}

fn route_attr(attrs: &[Attribute]) -> Result<Option<(String, String)>> {
    let route_attrs = ROUTE_METHODS
        .into_iter()
        .filter_map(|method| route_method_attr(attrs, method).map(|attr| (method, attr)))
        .collect::<Vec<_>>();

    if route_attrs.len() > 1 {
        bail!("route methods must declare exactly one HTTP method attribute");
    }

    let Some((method, attr)) = route_attrs.first() else {
        return Ok(None);
    };
    let path = attr.parse_args::<LitStr>().with_context(|| {
        format!("#[{method}] requires a string literal path like #[{method}(\"/:id\")]")
    })?;
    Ok(Some(((*method).to_owned(), path.value())))
}

fn route_method_attr<'a>(attrs: &'a [Attribute], method: &str) -> Option<&'a Attribute> {
    attrs.iter().find(|attr| attr.path().is_ident(method))
}

fn type_attrs(attrs: &[Attribute], name: &str) -> Result<Vec<String>> {
    let mut values = Vec::new();
    for attr in attrs.iter().filter(|attr| attr.path().is_ident(name)) {
        let path = attr.parse_args::<syn::Path>().with_context(|| {
            format!("#[{name}] requires a type path like #[{name}(ValidationPipe)]")
        })?;
        let Some(name) = type_path_name(&path) else {
            bail!("#[{name}] requires a simple type path without generic arguments");
        };
        values.push(name);
    }
    Ok(values)
}

fn openapi_attr(attrs: &[Attribute]) -> Result<Option<crate::source_openapi::OpenApiMetadata>> {
    let Some(attr) = attrs.iter().find(|attr| attr.path().is_ident("openapi")) else {
        return Ok(None);
    };
    let Meta::List(list) = &attr.meta else {
        bail!("#[openapi] requires summary = \"...\" metadata");
    };
    parse_openapi_args(&list.tokens.to_string()).map(Some)
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

fn impl_self_type_name(implementation: &ItemImpl) -> Option<String> {
    let Type::Path(self_ty) = &*implementation.self_ty else {
        return None;
    };
    self_ty
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn type_path_name(path: &syn::Path) -> Option<String> {
    let mut output = Vec::new();
    for segment in &path.segments {
        if !matches!(segment.arguments, PathArguments::None) {
            return None;
        }
        output.push(segment.ident.to_string());
    }
    (!output.is_empty()).then(|| output.join("::"))
}

fn has_unterminated_openapi_attr(contents: &str) -> bool {
    let mut remaining = contents;
    while let Some(start) = remaining.find("#[openapi(") {
        remaining = &remaining[start..];
        let Some(end) = remaining.find(")]") else {
            return true;
        };
        remaining = &remaining[end + 2..];
    }
    false
}

#[cfg(test)]
mod tests;

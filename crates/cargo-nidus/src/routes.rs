use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use crate::source_openapi::parse_openapi_args;

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
    sort_discovered_routes(&mut routes);
    Ok(routes)
}

pub(crate) fn openapi_path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let name = segment.strip_prefix('{')?.strip_suffix('}')?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
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
    let metadata = parse_openapi_args(args)?;
    *pending_summary = Some(metadata.summary);
    *pending_tags = metadata.tags;
    *pending_response_status = metadata.response_status;
    *pending_request_schema = metadata.request_schema;
    *pending_response_schema = metadata.response_schema;
    Ok(())
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

fn sort_discovered_routes(routes: &mut [DiscoveredRoute]) {
    routes.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| method_rank(&left.method).cmp(&method_rank(&right.method)))
            .then_with(|| left.method.cmp(&right.method))
    });
}

fn method_rank(method: &str) -> usize {
    ["get", "post", "put", "patch", "delete"]
        .iter()
        .position(|candidate| *candidate == method)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{DiscoveredRoute, sort_discovered_routes};

    #[test]
    fn discovered_routes_are_sorted_by_path_then_http_method() {
        let mut routes = vec![
            route("delete", "/users/{id}"),
            route("post", "/users"),
            route("get", "/health"),
            route("get", "/users"),
        ];

        sort_discovered_routes(&mut routes);

        let ordered = routes
            .into_iter()
            .map(|route| (route.method, route.path))
            .collect::<Vec<_>>();
        assert_eq!(
            ordered,
            [
                ("get".to_owned(), "/health".to_owned()),
                ("get".to_owned(), "/users".to_owned()),
                ("post".to_owned(), "/users".to_owned()),
                ("delete".to_owned(), "/users/{id}".to_owned()),
            ]
        );
    }

    fn route(method: &str, path: &str) -> DiscoveredRoute {
        DiscoveredRoute {
            method: method.to_owned(),
            path: path.to_owned(),
            summary: None,
            tags: Vec::new(),
            response_status: None,
            request_schema: None,
            response_schema: None,
            guards: Vec::new(),
            pipes: Vec::new(),
            validates: false,
        }
    }
}

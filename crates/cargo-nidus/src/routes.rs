use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use syn::{Attribute, ImplItem, Item, ItemImpl, ItemStruct, LitStr, Meta, PathArguments, Type};

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
        if has_unterminated_openapi_attr(&contents) {
            bail!("unterminated #[openapi] metadata");
        }
        let file =
            syn::parse_file(&contents).with_context(|| format!("parsing {}", path.display()))?;
        routes.extend(discover_controller_routes(&file)?);
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

fn discover_controller_routes(file: &syn::File) -> Result<Vec<DiscoveredRoute>> {
    let controller_prefixes = controller_prefixes(file);
    let mut routes = Vec::new();

    for item in &file.items {
        let Item::Impl(implementation) = item else {
            continue;
        };
        let Some(controller_name) = impl_self_type_name(implementation) else {
            continue;
        };
        let Some(prefix) = controller_prefixes.get(&controller_name) else {
            continue;
        };

        for item in &implementation.items {
            let ImplItem::Fn(function) = item else {
                continue;
            };
            let Some((method, route_path)) = route_attr(&function.attrs) else {
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
                guards: type_attrs(&function.attrs, "guard"),
                pipes: type_attrs(&function.attrs, "pipe"),
                validates: has_attr(&function.attrs, "validate"),
            });
        }
    }

    Ok(routes)
}

fn controller_prefixes(file: &syn::File) -> HashMap<String, String> {
    file.items
        .iter()
        .filter_map(|item| {
            let Item::Struct(item) = item else {
                return None;
            };
            controller_prefix(item).map(|prefix| (item.ident.to_string(), prefix))
        })
        .collect()
}

fn controller_prefix(item: &ItemStruct) -> Option<String> {
    string_attr(&item.attrs, "controller")
}

fn route_attr(attrs: &[Attribute]) -> Option<(String, String)> {
    for method in ["get", "post", "put", "patch", "delete"] {
        if let Some(path) = string_attr(attrs, method) {
            return Some((method.to_owned(), path));
        }
    }
    None
}

fn string_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|attr| attr.path().is_ident(name))?
        .parse_args::<LitStr>()
        .ok()
        .map(|value| value.value())
}

fn type_attrs(attrs: &[Attribute], name: &str) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident(name))
        .filter_map(|attr| attr.parse_args::<syn::Path>().ok())
        .filter_map(|path| type_path_name(&path))
        .collect()
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
    use super::{DiscoveredRoute, discover_controller_routes, sort_discovered_routes};

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

    #[test]
    fn discovers_routes_from_syn_attributes_in_controller_impls() {
        let file = syn::parse_file(
            r#"
use nidus::prelude::*;

#[controller("/users")]
pub struct UsersController;

#[routes]
impl UsersController {
    #[guard(crate::auth::AuthGuard)]
    #[pipe(ValidationPipe)]
    #[validate]
    #[openapi(
        summary = "Find user",
        tags = ["users", "read"],
        status = 201,
        request = CreateUserDto,
        response = UserDto
    )]
    #[get(
        "/:id"
    )]
    pub async fn find(&self) {}
}
"#,
        )
        .unwrap();

        let routes = discover_controller_routes(&file).unwrap();

        assert_eq!(routes.len(), 1);
        let route = &routes[0];
        assert_eq!(route.method, "get");
        assert_eq!(route.path, "/users/{id}");
        assert_eq!(route.summary.as_deref(), Some("Find user"));
        assert_eq!(route.tags, ["users", "read"]);
        assert_eq!(route.response_status, Some(201));
        assert_eq!(route.request_schema.as_deref(), Some("CreateUserDto"));
        assert_eq!(route.response_schema.as_deref(), Some("UserDto"));
        assert_eq!(route.guards, ["crate::auth::AuthGuard"]);
        assert_eq!(route.pipes, ["ValidationPipe"]);
        assert!(route.validates);
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

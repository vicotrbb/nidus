use anyhow::{Result, bail};

pub(crate) fn join_route(prefix: &str, route: &str) -> Result<String> {
    let prefix = normalize_path(prefix)?;
    let route = normalize_path(route)?;
    let prefix = trim_mount_suffix(&prefix);
    let joined = if prefix == "/" {
        route
    } else if route == "/" {
        prefix.to_owned()
    } else {
        format!("{prefix}{route}")
    };
    Ok(convert_nest_params(&joined))
}

pub(crate) fn openapi_path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let name = segment.strip_prefix('{')?.strip_suffix('}')?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
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

fn trim_mount_suffix(prefix: &str) -> &str {
    let trimmed = prefix.trim_end_matches('/');
    if trimmed.is_empty() { "/" } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::{join_route, openapi_path_parameters};

    #[test]
    fn join_route_normalizes_prefix_and_route_paths() {
        assert_eq!(join_route("users", ":id").unwrap(), "/users/{id}");
        assert_eq!(join_route("/", "health").unwrap(), "/health");
        assert_eq!(join_route("health", "/").unwrap(), "/health");
        assert_eq!(join_route("/users/", "/:id").unwrap(), "/users/{id}");
    }

    #[test]
    fn join_route_rejects_empty_parameter_names() {
        let error = join_route("/users", ":").unwrap_err();

        assert_eq!(
            error.to_string(),
            "route path `:` contains a parameter segment without a name after ':'"
        );
    }

    #[test]
    fn openapi_path_parameters_extract_braced_parameters() {
        assert_eq!(
            openapi_path_parameters("/users/{user_id}/posts/{post-id}"),
            ["user_id", "post-id"]
        );
    }
}

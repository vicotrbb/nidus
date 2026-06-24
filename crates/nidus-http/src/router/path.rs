use crate::error::RoutePathError;

pub(crate) fn join_paths(prefix: &str, path: &str) -> Result<String, RoutePathError> {
    let prefix = normalize_path(prefix)?;
    let path = normalize_path(path)?;
    let full_path = if prefix == "/" {
        path
    } else if path == "/" {
        prefix
    } else {
        format!("{prefix}{path}")
    };
    Ok(full_path)
}

pub(crate) fn normalize_path(path: impl AsRef<str>) -> Result<String, RoutePathError> {
    let path = path.as_ref().trim();
    validate_path(path)?;
    let with_slash = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    Ok(convert_nest_params(&with_slash))
}

fn validate_path(path: &str) -> Result<(), RoutePathError> {
    for segment in path.split('/') {
        if segment == ":" {
            return Err(RoutePathError::empty_parameter(path));
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

use crate::error::RoutePathError;

pub(crate) fn join_paths(prefix: &str, path: &str) -> Result<String, RoutePathError> {
    let prefix = normalize_mount_prefix(prefix)?;
    let path = normalize_path(path)?;
    Ok(join_normalized_paths(&prefix, &path))
}

pub(crate) fn normalize_mount_prefix(prefix: impl AsRef<str>) -> Result<String, RoutePathError> {
    let mut prefix = normalize_path(prefix)?;
    let trimmed_len = prefix.trim_end_matches('/').len();
    if trimmed_len == 0 {
        prefix.truncate(1);
    } else {
        prefix.truncate(trimmed_len);
    }
    Ok(prefix)
}

pub(crate) fn normalize_path(path: impl AsRef<str>) -> Result<String, RoutePathError> {
    let path = path.as_ref().trim();
    let parameter_count = validate_path(path)?;
    let leading_slash = usize::from(!path.starts_with('/'));
    let mut normalized = String::with_capacity(path.len() + leading_slash + parameter_count);
    if leading_slash == 1 {
        normalized.push('/');
    }

    for (index, segment) in path.split('/').enumerate() {
        if index > 0 {
            normalized.push('/');
        }
        if let Some(name) = segment.strip_prefix(':') {
            normalized.push('{');
            normalized.push_str(name);
            normalized.push('}');
        } else {
            normalized.push_str(segment);
        }
    }
    Ok(normalized)
}

pub(crate) fn join_normalized_paths(prefix: &str, path: &str) -> String {
    if prefix == "/" {
        return path.to_owned();
    }
    if path == "/" {
        return prefix.to_owned();
    }

    let mut full_path = String::with_capacity(prefix.len() + path.len());
    full_path.push_str(prefix);
    full_path.push_str(path);
    full_path
}

fn validate_path(path: &str) -> Result<usize, RoutePathError> {
    let mut parameter_count = 0;
    for segment in path.split('/') {
        if segment == ":" {
            return Err(RoutePathError::empty_parameter(path));
        }
        if segment.starts_with(':') {
            parameter_count += 1;
        }
    }
    Ok(parameter_count)
}

#[cfg(test)]
mod tests {
    use super::{join_normalized_paths, normalize_mount_prefix, normalize_path};

    #[test]
    fn normalize_path_preserves_structure_and_converts_parameters() {
        for (input, expected) in [
            ("", "/"),
            ("/", "/"),
            (
                " users/:id/posts/:post_id/ ",
                "/users/{id}/posts/{post_id}/",
            ),
            ("//users//:id", "//users//{id}"),
            ("/users/{id}", "/users/{id}"),
        ] {
            assert_eq!(normalize_path(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn normalized_path_join_handles_root_and_nested_routes() {
        assert_eq!(join_normalized_paths("/", "/users/{id}"), "/users/{id}");
        assert_eq!(normalize_mount_prefix("/health///").unwrap(), "/health");
        assert_eq!(join_normalized_paths("/health", "/"), "/health");
        assert_eq!(
            join_normalized_paths("/users", "/{id}/posts"),
            "/users/{id}/posts"
        );
    }
}

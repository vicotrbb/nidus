//! Shared route string operations for runtime routing and source inspection.
//!
//! Conversion preserves separators and whitespace. Normalization additionally
//! trims surrounding whitespace and inserts a leading slash.

/// Invalid route path containing an unnamed parameter.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("route path `{path}` contains a parameter segment without a name after ':'")]
pub struct RoutePathError {
    path: String,
}

impl RoutePathError {
    /// Creates an error for a route path parameter segment without a name.
    pub fn empty_parameter(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the invalid route path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Converts `:name` segments to `{name}` while preserving the path structure.
pub fn convert_parameters(path: &str) -> Result<String, RoutePathError> {
    let mut parameter_count = 0;
    for segment in path.split('/') {
        if segment == ":" {
            return Err(RoutePathError::empty_parameter(path));
        }
        if segment.starts_with(':') {
            parameter_count += 1;
        }
    }

    let mut normalized = String::with_capacity(path.len() + parameter_count);
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

/// Trims a route, adds its leading slash, and converts colon parameters.
pub fn normalize_path(path: impl AsRef<str>) -> Result<String, RoutePathError> {
    let path = path.as_ref().trim();
    let mut normalized = convert_parameters(path)?;
    if !normalized.starts_with('/') {
        normalized.insert(0, '/');
    }
    Ok(normalized)
}

/// Normalizes a mount prefix and route, then joins them.
pub fn join_paths(prefix: &str, path: &str) -> Result<String, RoutePathError> {
    let prefix = normalize_mount_prefix(prefix)?;
    let path = normalize_path(path)?;
    Ok(join_normalized_paths(&prefix, &path))
}

/// Normalizes a mount prefix and removes trailing slashes.
pub fn normalize_mount_prefix(prefix: impl AsRef<str>) -> Result<String, RoutePathError> {
    let mut prefix = normalize_path(prefix)?;
    let trimmed_len = prefix.trim_end_matches('/').len();
    if trimmed_len == 0 {
        prefix.truncate(1);
    } else {
        prefix.truncate(trimmed_len);
    }
    Ok(prefix)
}

/// Joins normalized paths, treating root paths specially.
pub fn join_normalized_paths(prefix: &str, path: &str) -> String {
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

/// Extracts names from braced path parameter segments.
pub fn openapi_path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let name = segment.strip_prefix('{')?.strip_suffix('}')?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

/// Builds a stable identifier from a method and path.
pub fn operation_id(method: &str, path: &str) -> String {
    let parameter_count = path.bytes().filter(|byte| *byte == b'{').count();
    let mut operation = String::with_capacity(method.len() + path.len() + parameter_count * 3 + 5);
    operation.push_str(method);
    let mut has_path_segment = false;
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        {
            operation.push_str("_by_");
            push_identifier_segment(&mut operation, name);
        } else {
            operation.push('_');
            push_identifier_segment(&mut operation, segment);
        }
        has_path_segment = true;
    }
    if !has_path_segment {
        operation.push_str("_root");
    }
    operation
}

fn push_identifier_segment(output: &mut String, segment: &str) {
    let start = output.len();
    let mut previous_was_separator = true;
    for character in segment.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }
    if output.len() > start && output.ends_with('_') {
        output.pop();
    }
    if output.len() == start {
        output.push_str("value");
    }
}

#[cfg(test)]
mod tests {
    use super::{convert_parameters, openapi_path_parameters, operation_id};

    #[test]
    fn openapi_path_normalizes_nidus_parameters() {
        assert_eq!(
            convert_parameters("/users/:user_id/posts/:post-id").unwrap(),
            "/users/{user_id}/posts/{post-id}"
        );
        assert_eq!(
            convert_parameters("//users//:id/").unwrap(),
            "//users//{id}/"
        );
    }

    #[test]
    fn conversion_preserves_whitespace_and_missing_leading_slash() {
        assert_eq!(convert_parameters(" users/:id/ ").unwrap(), " users/{id}/ ");
        assert_eq!(convert_parameters("").unwrap(), "");
    }

    #[test]
    fn openapi_path_rejects_empty_parameter_name() {
        let error = convert_parameters("/:").unwrap_err();

        assert_eq!(error.path(), "/:");
    }

    #[test]
    fn openapi_path_parameters_extract_braced_parameters() {
        assert_eq!(
            openapi_path_parameters("/users/{user_id}/posts/{post-id}"),
            ["user_id", "post-id"]
        );
    }

    #[test]
    fn operation_id_uses_stable_identifier_segments() {
        assert_eq!(
            operation_id("get", "/users/{user_id}/posts/{post-id}"),
            "get_users_by_user_id_posts_by_post_id"
        );
        assert_eq!(operation_id("get", "/"), "get_root");
        assert_eq!(operation_id("post", "/---/{...}"), "post_value_by_value");
    }
}

#[cfg(test)]
mod normalization_tests {
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

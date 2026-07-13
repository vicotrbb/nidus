use nidus_http::error::RoutePathError;

pub(crate) fn openapi_path(path: &str) -> Result<String, RoutePathError> {
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

pub(crate) fn openapi_path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let name = segment.strip_prefix('{')?.strip_suffix('}')?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

pub(crate) fn operation_id(method: &str, path: &str) -> String {
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
    use super::{openapi_path, openapi_path_parameters, operation_id};

    #[test]
    fn openapi_path_normalizes_nidus_parameters() {
        assert_eq!(
            openapi_path("/users/:user_id/posts/:post-id").unwrap(),
            "/users/{user_id}/posts/{post-id}"
        );
        assert_eq!(openapi_path("//users//:id/").unwrap(), "//users//{id}/");
    }

    #[test]
    fn openapi_path_rejects_empty_parameter_name() {
        let error = openapi_path("/:").unwrap_err();

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

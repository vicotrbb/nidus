use nidus_http::error::RoutePathError;

pub(crate) fn openapi_path(path: &str) -> Result<String, RoutePathError> {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        if segment == ":" {
            return Err(RoutePathError::empty_parameter(path));
        }
        if let Some(name) = segment.strip_prefix(':') {
            segments.push(format!("{{{name}}}"));
        } else {
            segments.push(segment.to_owned());
        }
    }
    Ok(segments.join("/"))
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
    let mut parts = vec![method.to_owned()];
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        {
            parts.push("by".to_owned());
            parts.push(identifier_segment(name));
        } else {
            parts.push(identifier_segment(segment));
        }
    }
    if parts.len() == 1 {
        parts.push("root".to_owned());
    }
    parts.join("_")
}

fn identifier_segment(segment: &str) -> String {
    let mut output = String::new();
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
    if output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "value".to_owned()
    } else {
        output
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
    }
}

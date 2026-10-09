use anyhow::Result;
pub(crate) use nidus_core::route::openapi_path_parameters;

pub(crate) fn join_route(prefix: &str, route: &str) -> Result<String> {
    Ok(nidus_core::route::join_paths(prefix, route)?)
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

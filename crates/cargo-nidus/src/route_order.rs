use crate::routes::DiscoveredRoute;

pub(crate) fn sort_discovered_routes(routes: &mut [DiscoveredRoute]) {
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
    use super::sort_discovered_routes;
    use crate::routes::DiscoveredRoute;

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

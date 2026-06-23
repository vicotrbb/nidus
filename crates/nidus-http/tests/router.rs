use nidus_http::router::RouteMetadata;

#[test]
fn route_metadata_composes_controller_prefix_with_normalized_path() {
    let metadata = RouteMetadata::new("GET", ":id");

    assert_eq!(metadata.full_path("/users"), "/users/{id}");
}

#[test]
fn route_metadata_composes_root_route_without_duplicate_slash() {
    let metadata = RouteMetadata::new("GET", "/");

    assert_eq!(metadata.full_path("/health"), "/health");
}

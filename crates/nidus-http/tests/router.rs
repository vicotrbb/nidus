use axum::body::{Body, to_bytes};
use http::{Method, Request, StatusCode};
use nidus_http::{controller::Controller, router::RouteDefinition, router::RouteMetadata};
use tower::ServiceExt;

#[test]
fn route_metadata_composes_controller_prefix_with_normalized_path() {
    let metadata = RouteMetadata::new("GET", ":id");

    assert_eq!(metadata.full_path("/users"), "/users/{id}");
    assert_eq!(metadata.full_path("/users/"), "/users/{id}");
}

#[test]
fn route_metadata_composes_root_route_without_duplicate_slash() {
    let metadata = RouteMetadata::new("GET", "/");

    assert_eq!(metadata.full_path("/health"), "/health");
    assert_eq!(metadata.full_path("/health/"), "/health");
}

#[test]
fn controller_try_new_normalizes_prefix() {
    let router = Controller::try_new("users")
        .unwrap()
        .route(RouteDefinition::get(":id", || async { "ok" }))
        .try_into_router();

    assert!(router.is_ok());
}

#[test]
fn controller_try_new_rejects_invalid_prefix() {
    let error = match Controller::try_new("/:") {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    assert_eq!(error.path(), "/:");
}

#[test]
fn route_definition_try_get_rejects_empty_parameter_name() {
    let error = match RouteDefinition::try_get("/:".to_owned(), || async { "ok" }) {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    assert_eq!(error.path(), "/:");
    assert_eq!(
        error.to_string(),
        "route path `/:` contains a parameter segment without a name after ':'"
    );
}

#[test]
fn route_metadata_try_full_path_rejects_invalid_controller_prefix() {
    let metadata = RouteMetadata::new("GET", "/");

    let error = match metadata.try_full_path("/:") {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    assert_eq!(error.path(), "/:");
}

#[test]
fn controller_try_into_router_rejects_invalid_prefix() {
    let error = match Controller::new("/:")
        .route(RouteDefinition::get("/", || async { "ok" }))
        .try_into_router()
    {
        Ok(_) => panic!("empty route parameter should fail"),
        Err(error) => error,
    };

    assert_eq!(error.path(), "/:");
}

#[tokio::test]
async fn controller_mounts_multiple_methods_at_the_same_path() {
    let router = Controller::new("/users")
        .route(RouteDefinition::get("/", || async { "listed" }))
        .route(RouteDefinition::post("/", || async { "created" }))
        .into_router();

    let get = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let post = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get.status(), StatusCode::OK);
    assert_eq!(post.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(get.into_body(), usize::MAX).await.unwrap(),
        "listed"
    );
    assert_eq!(
        to_bytes(post.into_body(), usize::MAX).await.unwrap(),
        "created"
    );
}

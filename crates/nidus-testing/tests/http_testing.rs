use axum::{
    Json, Router,
    response::IntoResponse,
    routing::{delete, get, head, patch, post, put},
};
use http::{StatusCode, header::HeaderName};
use nidus_http::{controller::Controller, router::RouteDefinition};
use nidus_testing::TestApp;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UserDto {
    id: u64,
    name: String,
}

#[tokio::test]
async fn route_definition_mounts_controller_prefix_on_axum_router() {
    let router = Controller::new("/users")
        .route(RouteDefinition::get("/:id", || async {
            Json(json!({ "id": 42 }))
        }))
        .into_router();

    let response = TestApp::from_router(router).get("/users/42").send().await;

    response.assert_status(http::StatusCode::OK);
    response.assert_json(json!({ "id": 42 })).await;
}

#[tokio::test]
async fn test_response_exposes_status_body_and_typed_json() {
    let router = Router::new().route(
        "/users",
        post(|| async { (StatusCode::CREATED, Json(json!({ "id": 7, "name": "Ada" }))) }),
    );

    let response = TestApp::from_router(router)
        .post("/users")
        .json(&json!({ "name": "Ada" }))
        .send()
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(!response.body().is_empty());
    assert_eq!(
        response.json::<UserDto>(),
        UserDto {
            id: 7,
            name: "Ada".to_owned(),
        }
    );
    assert_eq!(
        response.try_json::<UserDto>().unwrap(),
        UserDto {
            id: 7,
            name: "Ada".to_owned(),
        }
    );
}

#[tokio::test]
async fn test_response_exposes_text_and_fallible_json_decode() {
    let router = Router::new().route("/health", get(|| async { "ok" }));

    let response = TestApp::from_router(router).get("/health").send().await;

    assert_eq!(response.text().unwrap(), "ok");
    assert!(response.try_json::<UserDto>().is_err());
}

#[tokio::test]
async fn test_response_exposes_and_asserts_headers() {
    let router = Router::new().route(
        "/health",
        get(|| async {
            ([(HeaderName::from_static("x-request-id"), "req-123")], "ok").into_response()
        }),
    );

    let response = TestApp::from_router(router).get("/health").send().await;

    response.assert_status(StatusCode::OK);
    response.assert_header("x-request-id", "req-123");
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("req-123")
    );
    assert_eq!(
        response
            .header("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("req-123")
    );
    assert_eq!(
        response.header_str("x-request-id").unwrap(),
        Some("req-123")
    );
    assert_eq!(response.header_str("x-missing").unwrap(), None);
}

#[tokio::test]
async fn route_definition_mounts_common_mutation_verbs() {
    let router = Controller::new("/users")
        .route(RouteDefinition::put("/:id", || async { "put" }))
        .route(RouteDefinition::patch("/:id", || async { "patch" }))
        .route(RouteDefinition::delete("/:id", || async { "delete" }))
        .into_router();
    let app = TestApp::from_router(router);

    app.put("/users/42").send().await.assert_text("put").await;
    app.patch("/users/42")
        .send()
        .await
        .assert_text("patch")
        .await;
    app.delete("/users/42")
        .send()
        .await
        .assert_text("delete")
        .await;
}

#[tokio::test]
async fn test_app_can_wrap_plain_axum_router() {
    let router = Router::new().route("/health", get(|| async { "ok" }));

    let response = TestApp::from_router(router).get("/health").send().await;

    response.assert_status(http::StatusCode::OK);
    response.assert_text("ok").await;
}

#[tokio::test]
async fn test_app_can_send_arbitrary_http_methods() {
    let router = Router::new().route("/health", head(|| async { "" }));

    let response = TestApp::from_router(router)
        .request(http::Method::HEAD, "/health")
        .send()
        .await;

    response.assert_status(StatusCode::OK);
    assert!(response.body().is_empty());
}

#[tokio::test]
async fn test_app_request_helpers_cover_common_mutation_verbs() {
    let router = Router::new()
        .route("/users/42", put(|| async { "put" }))
        .route("/users/42", patch(|| async { "patch" }))
        .route("/users/42", delete(|| async { "delete" }));
    let app = TestApp::from_router(router);

    app.put("/users/42").send().await.assert_text("put").await;
    app.patch("/users/42")
        .send()
        .await
        .assert_text("patch")
        .await;
    app.delete("/users/42")
        .send()
        .await
        .assert_text("delete")
        .await;
}

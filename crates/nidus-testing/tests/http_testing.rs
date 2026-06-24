use axum::{
    Json, Router,
    body::Bytes,
    extract::Query,
    response::IntoResponse,
    routing::{delete, get, head, patch, post, put},
};
use http::{HeaderMap, StatusCode, header::HeaderName};
use nidus_http::{controller::Controller, router::RouteDefinition};
use nidus_testing::TestApp;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct UserDto {
    id: u64,
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct SearchQuery {
    name: String,
    page: u32,
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
async fn test_request_sends_custom_headers() {
    let router = Router::new().route(
        "/echo-header",
        post(|headers: HeaderMap| async move {
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing")
                .to_owned()
        }),
    );

    let response = TestApp::from_router(router)
        .post("/echo-header")
        .header("x-api-key", "secret")
        .send()
        .await;

    response.assert_text("secret").await;
}

#[test]
fn test_request_try_header_reports_invalid_headers() {
    let router = Router::new();

    let invalid_name = TestApp::from_router(router.clone())
        .get("/health")
        .try_header("bad header", "secret");
    assert!(invalid_name.is_err());

    let invalid_value = TestApp::from_router(router)
        .get("/health")
        .try_header("x-api-key", "bad\nvalue");
    assert!(invalid_value.is_err());
}

#[tokio::test]
async fn test_request_sends_text_body_with_content_type() {
    let router = Router::new().route(
        "/echo-text",
        post(|headers: HeaderMap, body: String| async move {
            let content_type = headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing");
            format!("{content_type}:{body}")
        }),
    );

    let response = TestApp::from_router(router)
        .post("/echo-text")
        .text("hello")
        .send()
        .await;

    response
        .assert_text("text/plain; charset=utf-8:hello")
        .await;
}

#[tokio::test]
async fn test_request_sends_raw_body() {
    let router = Router::new().route(
        "/bytes",
        post(|body: Bytes| async move { body.len().to_string() }),
    );

    let response = TestApp::from_router(router)
        .post("/bytes")
        .body(Bytes::from_static(b"nidus"))
        .send()
        .await;

    response.assert_text("5").await;
}

#[tokio::test]
async fn test_request_appends_typed_query_params() {
    let router =
        Router::new().route(
            "/users",
            get(|Query(query): Query<SearchQuery>| async move {
                format!("{}:{}", query.name, query.page)
            }),
        );

    let response = TestApp::from_router(router)
        .get("/users")
        .query(&SearchQuery {
            name: "Ada Lovelace".to_owned(),
            page: 2,
        })
        .send()
        .await;

    response.assert_text("Ada Lovelace:2").await;
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

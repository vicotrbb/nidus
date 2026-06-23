use axum::{Json, Router, routing::get};
use nidus_http::{controller::Controller, router::RouteDefinition};
use nidus_testing::TestApp;
use serde_json::json;

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
async fn test_app_can_wrap_plain_axum_router() {
    let router = Router::new().route("/health", get(|| async { "ok" }));

    let response = TestApp::from_router(router).get("/health").send().await;

    response.assert_status(http::StatusCode::OK);
    response.assert_text("ok").await;
}

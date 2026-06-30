#![cfg(feature = "dashboard")]

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use nidus::prelude::*;
use tower::ServiceExt;

#[module]
struct AppModule;

#[tokio::test]
async fn module_builder_mounts_dashboard_router() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    let app = Nidus::create::<AppModule>()
        .with_dashboard(dashboard)
        .build()
        .await
        .unwrap()
        .into_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nidus/dashboard/")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("Nidus Dashboard"));
}

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/{id}")]
    async fn show(&self) -> &'static str {
        "user"
    }
}

#[module(controllers(UsersController))]
struct RoutesModule;

#[tokio::test]
async fn dashboard_routes_api_includes_nidus_controller_routes() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    let app = Nidus::create::<RoutesModule>()
        .with_dashboard(dashboard)
        .build()
        .await
        .unwrap()
        .into_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nidus/dashboard/api/routes")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("\"method\":\"GET\""), "{text}");
    assert!(text.contains("\"path\":\"/users/{id}\""), "{text}");
}

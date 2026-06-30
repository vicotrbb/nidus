use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};
use tower::ServiceExt;

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", "Bearer secret")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn dashboard_serves_shell_and_overview_api() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let shell = app.clone().oneshot(request("/")).await.unwrap();
    assert_eq!(shell.status(), StatusCode::OK);
    let body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("Nidus Dashboard"));

    let overview = app.oneshot(request("/api/overview")).await.unwrap();
    assert_eq!(overview.status(), StatusCode::OK);
    let body = to_bytes(overview.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("\"service_name\""));
}

#[tokio::test]
async fn dashboard_serves_assets_under_assets_path() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let styles = app.oneshot(request("/assets/styles.css")).await.unwrap();

    assert_eq!(styles.status(), StatusCode::OK);
}

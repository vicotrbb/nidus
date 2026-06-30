use axum::body::Body;
use http::{Request, StatusCode};
use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};
use tower::ServiceExt;

#[tokio::test]
async fn dashboard_rejects_missing_bearer_token() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_rejects_invalid_bearer_token() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_accepts_valid_bearer_token() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

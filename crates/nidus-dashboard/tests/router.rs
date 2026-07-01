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
async fn dashboard_serves_graph_api_contract() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    dashboard.record_graph_snapshot(nidus_dashboard::DashboardGraphResponse {
        service_name: "catalog-api".to_owned(),
        generated_at_ms: 1_725_000_000_000,
        nodes: vec![nidus_dashboard::DashboardGraphNode {
            id: "module:CatalogModule".to_owned(),
            kind: nidus_dashboard::DashboardGraphNodeKind::Module,
            label: "CatalogModule".to_owned(),
            summary: Some("1 controller".to_owned()),
            status: None,
            counts: Default::default(),
            metadata: Default::default(),
        }],
        edges: vec![],
        groups: vec![],
    });

    let response = dashboard
        .router()
        .oneshot(request("/api/graph"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("\"service_name\":\"catalog-api\""), "{body}");
    assert!(body.contains("\"kind\":\"module\""), "{body}");
    assert!(body.contains("CatalogModule"), "{body}");
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

#[tokio::test]
async fn dashboard_serves_event_job_and_adapter_api_views() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_event("user.created", Some("event-1"), Vec::<(&str, &str)>::new())
        .await
        .unwrap();
    dashboard
        .collector()
        .record_job("daily_digest", Some("job-1"), true, 12)
        .await
        .unwrap();

    let app = dashboard.router();

    let events = app.clone().oneshot(request("/api/events")).await.unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let body = to_bytes(events.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("user.created"));
    assert!(!body.contains("daily_digest"));

    let jobs = app.clone().oneshot(request("/api/jobs")).await.unwrap();
    assert_eq!(jobs.status(), StatusCode::OK);
    let body = to_bytes(jobs.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("daily_digest"));
    assert!(!body.contains("user.created"));

    let adapters = app.oneshot(request("/api/adapters")).await.unwrap();
    assert_eq!(adapters.status(), StatusCode::OK);
}

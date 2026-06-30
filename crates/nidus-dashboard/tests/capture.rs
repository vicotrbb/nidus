use nidus_dashboard::{
    DashboardAuth, DashboardOperationKind, DashboardStorage, NidusDashboard,
    storage::DashboardStorageBackend,
};

#[tokio::test]
async fn dashboard_collector_records_metadata_without_payloads() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_event("user.created", Some("op-1"), [("tenant", "acme")])
        .await
        .unwrap();

    let operations = dashboard.storage().list_operations(10).await.unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind, DashboardOperationKind::Event);
    assert_eq!(operations[0].name, "user.created");
    assert_eq!(operations[0].payload, None);
    assert_eq!(
        operations[0].attributes.get("tenant").map(String::as_str),
        Some("acme")
    );
}

#[tokio::test]
async fn payload_capture_redacts_configured_fields() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .capture(
            nidus_dashboard::DashboardCapture::payloads()
                .redact_fields(["password", "token"])
                .max_payload_bytes(1024),
        )
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_payload_event(
            "user.login",
            Some("op-2"),
            serde_json::json!({
                "email": "user@example.com",
                "password": "secret",
                "nested": { "token": "abc" }
            }),
        )
        .await
        .unwrap();

    let operations = dashboard.storage().list_operations(10).await.unwrap();
    let payload = operations[0].payload.as_ref().unwrap();

    assert_eq!(payload["email"], "user@example.com");
    assert_eq!(payload["password"], "[redacted]");
    assert_eq!(payload["nested"]["token"], "[redacted]");
}

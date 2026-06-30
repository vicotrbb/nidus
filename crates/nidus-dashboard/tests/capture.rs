use nidus_dashboard::{
    DashboardAuth, DashboardOperationKind, DashboardStorage, NidusDashboard,
    storage::{DashboardStorageBackend, SqliteDashboardStorage},
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

#[cfg(feature = "sqlite")]
#[tokio::test]
async fn dashboard_uses_configured_sqlite_storage() {
    let database_path =
        std::env::temp_dir().join(format!("nidus-dashboard-{}.sqlite", uuid::Uuid::new_v4()));
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::sqlite(&database_url))
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_event(
            "sqlite.persisted",
            Some("op-sqlite"),
            Vec::<(&str, &str)>::new(),
        )
        .await
        .unwrap();

    let sqlite = SqliteDashboardStorage::connect(&database_url)
        .await
        .unwrap();
    let operations = sqlite.list_operations(10).await.unwrap();

    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].id, "op-sqlite");
}

#[tokio::test]
async fn dashboard_collector_records_job_metadata() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_job("daily_digest", Some("run-1"), true, 42)
        .await
        .unwrap();

    let operations = dashboard.storage().list_operations(10).await.unwrap();

    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind, DashboardOperationKind::Job);
    assert_eq!(operations[0].name, "daily_digest");
    assert_eq!(operations[0].correlation_id.as_deref(), Some("run-1"));
    assert_eq!(operations[0].duration_ms, Some(42));
}

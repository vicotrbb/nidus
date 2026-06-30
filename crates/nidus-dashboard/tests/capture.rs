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

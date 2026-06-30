use std::collections::BTreeMap;

use nidus_dashboard::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus,
    storage::{DashboardStorageBackend, MemoryDashboardStorage},
};

fn operation(id: &str, name: &str) -> DashboardOperation {
    DashboardOperation {
        id: id.to_owned(),
        kind: DashboardOperationKind::Event,
        name: name.to_owned(),
        status: DashboardOperationStatus::Success,
        timestamp_ms: 1_725_000_000_000,
        duration_ms: Some(12),
        correlation_id: Some("req-1".to_owned()),
        attributes: BTreeMap::from([("source".to_owned(), "test".to_owned())]),
        payload: None,
    }
}

#[tokio::test]
async fn memory_storage_records_and_lists_timeline_operations() {
    let storage = MemoryDashboardStorage::new();

    storage
        .record_operation(operation("op-1", "user.created"))
        .await
        .unwrap();
    storage
        .record_operation(operation("op-2", "project.created"))
        .await
        .unwrap();

    let timeline = storage.list_operations(10).await.unwrap();

    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0].id, "op-2");
    assert_eq!(timeline[1].id, "op-1");
}

#[tokio::test]
async fn memory_storage_prunes_to_max_events() {
    let storage = MemoryDashboardStorage::new();

    storage
        .record_operation(operation("op-1", "first"))
        .await
        .unwrap();
    storage
        .record_operation(operation("op-2", "second"))
        .await
        .unwrap();
    storage.prune(1).await.unwrap();

    let timeline = storage.list_operations(10).await.unwrap();

    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].id, "op-2");
}

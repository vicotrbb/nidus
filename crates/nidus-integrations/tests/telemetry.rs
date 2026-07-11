use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use nidus_integrations::{
    IntegrationEvent, IntegrationObserver, IntegrationStatus, IntegrationTelemetry,
};

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<IntegrationEvent>>>);

#[async_trait]
impl IntegrationObserver for Recorder {
    async fn record(&self, event: &IntegrationEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

struct PanickingObserver;

#[async_trait]
impl IntegrationObserver for PanickingObserver {
    async fn record(&self, _event: &IntegrationEvent) {
        panic!("observer failure");
    }
}

#[tokio::test]
async fn composite_telemetry_records_stable_redaction_safe_events() {
    let recorder = Recorder::default();
    let telemetry = IntegrationTelemetry::new().with_observer(recorder.clone());
    let event = IntegrationEvent::new(
        "nidus-test",
        "publish",
        IntegrationStatus::Success,
        Duration::from_millis(3),
    )
    .correlation_id("request-1")
    .unwrap();

    telemetry.record(&event).await;
    assert_eq!(recorder.0.lock().unwrap().as_slice(), [event]);
}

#[tokio::test]
async fn panicking_observers_are_isolated_from_adapter_operations() {
    let recorder = Recorder::default();
    let telemetry = IntegrationTelemetry::new()
        .with_observer(PanickingObserver)
        .with_observer(recorder.clone());
    telemetry
        .record(&IntegrationEvent::new(
            "nidus-test",
            "health",
            IntegrationStatus::Success,
            Duration::from_millis(1),
        ))
        .await;
    assert_eq!(recorder.0.lock().unwrap().len(), 1);
}

#[cfg(feature = "dashboard")]
#[tokio::test]
async fn dashboard_bridge_records_adapter_timeline_operations() {
    use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};

    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("test-token"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();
    let storage = dashboard.storage();
    let telemetry = IntegrationTelemetry::new().dashboard(dashboard.collector());

    telemetry
        .record(&IntegrationEvent::new(
            "nidus-test",
            "health",
            IntegrationStatus::Failure,
            Duration::from_millis(7),
        ))
        .await;

    use nidus_dashboard::storage::DashboardStorageBackend;
    let operations = storage.list_operations(10).await.unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].name, "nidus-test.health");
}

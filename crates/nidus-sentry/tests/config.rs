use std::time::Duration;

use nidus_sentry::SentryConfig;

#[test]
fn enforces_secure_dsn_and_bounded_sampling() {
    let config = SentryConfig::new("https://public:secret@sentry.example.test/1")
        .with_environment("test")
        .with_release("service@1.0.0")
        .with_sample_rates(1.0, 0.25)
        .unwrap()
        .with_shutdown_timeout(Duration::from_secs(3))
        .unwrap();
    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("public"));
    assert!(!debug.contains("secret"));

    assert!(
        SentryConfig::new("http://public@sentry.example.test/1")
            .allow_insecure_local_dsn()
            .is_err()
    );
    assert!(
        SentryConfig::new("https://public@sentry.example.test/1")
            .with_sample_rates(1.1, 0.0)
            .is_err()
    );
    assert!(
        SentryConfig::new("https://public@sentry.example.test/1")
            .with_deduplication(Duration::ZERO, 1)
            .is_err()
    );
}

#[test]
fn permits_explicit_loopback_http_for_hermetic_development() {
    let config = SentryConfig::new("http://public@127.0.0.1:9000/1")
        .allow_insecure_local_dsn()
        .unwrap();
    assert!(format!("{config:?}").contains("allow_insecure_local_dsn: true"));
}

#[cfg(all(feature = "dashboard", feature = "health"))]
#[tokio::test]
async fn composes_with_health_di_lifecycle_and_dashboard() {
    use std::sync::Arc;

    use nidus_dashboard::storage::DashboardStorageBackend;
    use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};
    use nidus_sentry::SentryIntegration;

    let integration = Arc::new(
        SentryIntegration::init(SentryConfig::new("https://public@sentry.invalid/1")).unwrap(),
    );
    let _health = Arc::clone(&integration)
        .register_ready_check(nidus_http::health::HealthRegistry::new(), "sentry");
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("test-token"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();
    integration
        .record_dashboard_status(&dashboard.collector())
        .await
        .unwrap();
    let operations = dashboard.storage().list_operations(10).await.unwrap();
    assert_eq!(operations[0].name, "nidus-sentry.readiness");
    integration.shutdown().await.unwrap();
}

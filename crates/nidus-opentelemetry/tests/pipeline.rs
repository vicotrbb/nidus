use std::time::Duration;

use http::HeaderMap;
use nidus_opentelemetry::{OpenTelemetryConfig, OpenTelemetryPipeline, OtlpProtocol};
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use tracing_subscriber::prelude::*;

#[tokio::test]
async fn exports_tracing_spans_in_batches_and_flushes_without_global_state() {
    let exporter = InMemorySpanExporter::default();
    let config = OpenTelemetryConfig::grpc("orders", "https://collector.example.test:4317")
        .with_service_version("1.2.3")
        .with_environment("test")
        .with_batching(64, 16, Duration::from_secs(30))
        .unwrap();
    let pipeline = OpenTelemetryPipeline::from_exporter(config, exporter.clone()).unwrap();
    let subscriber = tracing_subscriber::registry().with(pipeline.tracing_layer());

    let mut headers = HeaderMap::new();
    {
        let _subscriber = tracing::subscriber::set_default(subscriber);
        let span = tracing::info_span!("order.submit", order.kind = "standard");
        let _entered = span.enter();
        tracing::info!("order accepted");
        pipeline.inject_current_context(&mut headers);
    }

    assert!(headers.contains_key("traceparent"));
    let extracted = pipeline.extract_context(&headers);
    assert!(extracted.span().span_context().is_valid());
    pipeline.force_flush().await.unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].name, "order.submit");

    pipeline.shutdown().await.unwrap();
    assert!(!pipeline.is_ready());
    pipeline.shutdown().await.unwrap();
}

#[test]
fn validates_secure_endpoints_bounds_and_redacts_headers() {
    let config =
        OpenTelemetryConfig::http_protobuf("billing", "https://collector.example.test/v1/traces")
            .with_header("authorization", "Bearer secret")
            .unwrap()
            .with_sample_ratio(0.25)
            .unwrap();
    assert_eq!(config.protocol(), OtlpProtocol::HttpProtobuf);
    let debug = format!("{config:?}");
    assert!(debug.contains("authorization"));
    assert!(!debug.contains("Bearer secret"));
    assert!(!debug.contains("collector.example.test"));
    assert!(debug.contains("endpoint: \"<redacted>\""));

    assert!(
        OpenTelemetryConfig::grpc("service", "http://collector.example.test:4317")
            .allow_insecure_local_endpoint()
            .is_err()
    );
    assert!(
        OpenTelemetryConfig::grpc("service", "http://localhost:4317@evil.example:4317")
            .allow_insecure_local_endpoint()
            .is_err()
    );
    assert!(
        OpenTelemetryConfig::grpc("service", "https://collector.example.test:4317")
            .with_sample_ratio(1.1)
            .is_err()
    );
    assert!(
        OpenTelemetryConfig::grpc("service", "https://collector.example.test:4317")
            .with_batching(4, 8, Duration::from_secs(1))
            .is_err()
    );
}

#[tokio::test]
async fn constructs_both_real_otlp_exporters_without_contacting_the_network() {
    let grpc = OpenTelemetryPipeline::init(
        OpenTelemetryConfig::grpc("grpc-test", "http://127.0.0.1:4317")
            .allow_insecure_local_endpoint()
            .unwrap(),
    )
    .unwrap();
    grpc.shutdown().await.unwrap();

    let http = OpenTelemetryPipeline::init(
        OpenTelemetryConfig::http_protobuf("http-test", "http://127.0.0.1:4318/v1/traces")
            .allow_insecure_local_endpoint()
            .unwrap(),
    )
    .unwrap();
    http.shutdown().await.unwrap();
}

#[cfg(all(feature = "dashboard", feature = "health"))]
#[tokio::test]
async fn composes_with_health_di_lifecycle_and_dashboard() {
    use std::sync::Arc;

    use nidus_dashboard::storage::DashboardStorageBackend;
    use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};

    let pipeline = Arc::new(
        OpenTelemetryPipeline::from_exporter(
            OpenTelemetryConfig::grpc("composition", "https://collector.example.test:4317"),
            InMemorySpanExporter::default(),
        )
        .unwrap(),
    );
    let _health = Arc::clone(&pipeline)
        .register_ready_check(nidus_http::health::HealthRegistry::new(), "opentelemetry");
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("test-token"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();
    pipeline
        .record_dashboard_status(&dashboard.collector())
        .await
        .unwrap();
    let operations = dashboard.storage().list_operations(10).await.unwrap();
    assert_eq!(operations[0].name, "nidus-opentelemetry.readiness");
    pipeline.shutdown().await.unwrap();
}

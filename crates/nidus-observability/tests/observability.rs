use std::time::Duration;

use axum::{Router, body::Body, routing::get};
use http::{Request, StatusCode};
use nidus_events::{EventBus, ObservedEventBus};
use nidus_jobs::{AsyncJob, Job, ObservedJobRunner};
use nidus_observability::{Observability, OperationStatus};
use tower::ServiceExt;

#[derive(Clone)]
struct UserCreated;

struct SuccessfulJob;

impl Job for SuccessfulJob {
    fn name(&self) -> &'static str {
        "successful_job"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

struct AsyncFailingJob;

#[async_trait::async_trait]
impl AsyncJob for AsyncFailingJob {
    fn name(&self) -> &'static str {
        "async_failing_job"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        Err(nidus_jobs::JobError::new("failed"))
    }
}

#[test]
fn renders_event_job_lifecycle_and_adapter_metrics() {
    let observability = Observability::production("users-api")
        .prometheus()
        .max_series(10);

    let events = ObservedEventBus::new(EventBus::new(), observability.event_observer());
    events.publish_named("user.created", UserCreated);

    let runner = ObservedJobRunner::new(observability.job_observer())
        .run_id_generator(|| "run-1".to_owned());
    runner.run(&SuccessfulJob).expect("job should succeed");

    observability.record_lifecycle_operation(
        "lifecycle.startup",
        OperationStatus::Success,
        Duration::from_millis(12),
    );
    observability.adapter_observer().record(
        "nidus-cache",
        "get",
        OperationStatus::Failure,
        Duration::from_millis(3),
    );

    let rendered = observability.render_prometheus();
    assert!(rendered.contains(r#"nidus_events_published_total{event="user.created"} 1"#));
    assert!(rendered.contains(r#"nidus_jobs_started_total{job="successful_job"} 1"#));
    assert!(
        rendered.contains(r#"nidus_jobs_finished_total{job="successful_job",status="success"} 1"#)
    );
    assert!(rendered.contains("nidus_job_duration_seconds_count"));
    assert!(
        rendered
            .contains(r#"nidus_lifecycle_total{operation="lifecycle.startup",status="success"} 1"#)
    );
    assert!(
        rendered.contains(
            r#"nidus_adapter_operations_total{adapter="nidus-cache",operation="get",status="failure"} 1"#
        )
    );
}

#[test]
fn observability_builds_observed_event_bus_and_job_runner_helpers() {
    let observability = Observability::production("users-api")
        .prometheus()
        .max_series(10);

    observability
        .observed_event_bus::<UserCreated>()
        .publish_named("user.created", UserCreated);
    observability
        .job_runner()
        .run(&SuccessfulJob)
        .expect("job should succeed");

    let rendered = observability.render_prometheus();
    assert!(rendered.contains(r#"nidus_events_published_total{event="user.created"} 1"#));
    assert!(rendered.contains(r#"nidus_jobs_started_total{job="successful_job"} 1"#));
}

#[tokio::test]
async fn records_async_job_failures() {
    let observability = Observability::production("users-api").prometheus();
    let runner = ObservedJobRunner::new(observability.job_observer())
        .run_id_generator(|| "run-2".to_owned());

    let result = runner.run_async(&AsyncFailingJob).await;

    assert!(result.is_err());
    let rendered = observability.render_prometheus();
    assert!(
        rendered
            .contains(r#"nidus_jobs_finished_total{job="async_failing_job",status="failure"} 1"#)
    );
}

#[test]
fn caps_non_http_metric_cardinality_with_overflow_label() {
    let observability = Observability::production("users-api")
        .prometheus()
        .max_series(1);
    let observer = observability.event_observer();

    ObservedEventBus::new(EventBus::new(), observer.clone())
        .publish_named("first.event", UserCreated);
    ObservedEventBus::new(EventBus::new(), observer).publish_named("second.event", UserCreated);

    let rendered = observability.render_prometheus();
    assert!(rendered.contains(r#"nidus_events_published_total{event="first.event"} 1"#));
    assert!(rendered.contains(r#"nidus_events_published_total{event="<overflow>"} 1"#));
}

#[test]
fn disabled_surfaces_do_not_emit_metrics() {
    let observability = Observability::production("users-api")
        .prometheus()
        .without_event_metrics()
        .without_job_metrics()
        .without_adapter_instrumentation();

    ObservedEventBus::new(EventBus::new(), observability.event_observer())
        .publish_named("user.created", UserCreated);
    let runner = ObservedJobRunner::new(observability.job_observer());
    runner.run(&SuccessfulJob).expect("job should succeed");
    observability.adapter_observer().record(
        "nidus-cache",
        "get",
        OperationStatus::Success,
        Duration::from_millis(1),
    );

    let rendered = observability.render_prometheus();
    assert!(!rendered.contains("nidus_events_published_total{event="));
    assert!(!rendered.contains("nidus_jobs_started_total{job="));
    assert!(!rendered.contains("nidus_adapter_operations_total{adapter="));
}

#[tokio::test]
async fn http_layer_and_routes_preserve_existing_http_metric_names() {
    let observability = Observability::production("users-api")
        .prometheus()
        .exclude_route("/metrics");
    let app = Router::new()
        .route("/ok", get(|| async { StatusCode::NO_CONTENT }))
        .layer(observability.http_layer())
        .merge(observability.routes());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ok")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("metrics request should complete");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let rendered = String::from_utf8(body.to_vec()).expect("body should be utf8");

    assert!(rendered.contains("# TYPE nidus_http_requests_total counter"));
    assert!(
        rendered.contains(r#"nidus_http_requests_total{method="GET",route="/ok",status="204"} 1"#)
    );
    assert!(rendered.contains("# TYPE nidus_http_request_duration_seconds histogram"));
    assert!(rendered.contains("# TYPE nidus_http_in_flight_requests gauge"));
    assert!(rendered.contains("# TYPE nidus_http_errors_total counter"));
}

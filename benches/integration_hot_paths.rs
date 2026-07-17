use std::{hint::black_box, time::Duration};

use criterion::{Criterion, criterion_group, criterion_main};
use nidus_events::{EventBus, EventObserver, ObservedEventContext};
use nidus_integrations::{EnvelopeMetadata, MessageEnvelope};
use nidus_jobs::{
    Job, JobObserver, JobResultStatus, JobRetryPolicy, NewJob, ObservedJobContext,
    ObservedJobRunner,
};
use nidus_observability::{Observability, OperationStatus};
use serde_json::{Value, json};

struct BenchmarkJob;

impl Job for BenchmarkJob {
    fn name(&self) -> &'static str {
        "benchmark_job"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct BenchmarkEventObserver;

impl EventObserver<u64> for BenchmarkEventObserver {
    fn on_event_published(&self, context: &ObservedEventContext) {
        black_box(context.operation_id());
        black_box(context.event_name());
        black_box(context.attributes());
    }
}

#[derive(Clone, Copy)]
struct BenchmarkJobObserver;

impl JobObserver for BenchmarkJobObserver {
    fn on_job_started(&self, context: &ObservedJobContext) {
        black_box(context.run_id());
        black_box(context.job_name());
        black_box(context.attributes());
    }

    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
        black_box(context.run_id());
        black_box(context.job_name());
        black_box(context.attributes());
        black_box(context.duration());
        black_box(status);
    }
}

fn integration_hot_paths(c: &mut Criterion) {
    let payload = json!({
        "order_id": 42,
        "data": "x".repeat(1_024),
    });
    let metadata = EnvelopeMetadata::new()
        .correlation_id("request-42")
        .unwrap()
        .traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
        .unwrap();
    let envelope = MessageEnvelope::new("orders.created", payload.clone())
        .unwrap()
        .with_metadata(metadata);
    let encoded = envelope.to_json().unwrap();

    c.bench_function("integration envelope serialize 1KiB", |b| {
        b.iter(|| black_box(&envelope).to_json().unwrap());
    });
    c.bench_function("integration envelope deserialize 1KiB", |b| {
        b.iter(|| {
            MessageEnvelope::<Value>::from_json(black_box(&encoded)).unwrap();
        });
    });
    c.bench_function("durable job validate and construct 1KiB", |b| {
        b.iter(|| NewJob::new("orders.process", black_box(payload.clone())).unwrap());
    });

    let retry = JobRetryPolicy::new(Duration::from_millis(25), Duration::from_secs(30)).unwrap();
    c.bench_function("durable job retry bound calculation", |b| {
        b.iter(|| retry.maximum_delay(black_box(8)));
    });

    let observability = Observability::production("benchmark")
        .prometheus()
        .max_series(64);
    let adapter_observer = observability.adapter_observer();
    let event_observer = observability.event_observer();
    let event_context = ObservedEventContext::new("benchmark-run", "orders.created");

    c.bench_function("observability lifecycle record", |b| {
        b.iter(|| {
            observability.record_lifecycle_operation(
                black_box("module.graph.validate"),
                black_box(OperationStatus::Success),
                black_box(Duration::from_millis(1)),
            );
        });
    });
    c.bench_function("observability adapter record", |b| {
        b.iter(|| {
            adapter_observer.record(
                black_box("nidus-sqlx"),
                black_box("acquire"),
                black_box(OperationStatus::Success),
                black_box(Duration::from_millis(1)),
            );
        });
    });
    c.bench_function("observability event record", |b| {
        b.iter(|| {
            EventObserver::<()>::on_event_published(
                black_box(&event_observer),
                black_box(&event_context),
            );
        });
    });

    let unconfigured_events = EventBus::<u64>::new()
        .observed(BenchmarkEventObserver)
        .operation_id_generator(|| "benchmark-event".to_owned());
    c.bench_function("observed event publish with 0 attributes", |b| {
        b.iter(|| {
            unconfigured_events.publish_named(black_box("benchmark.event"), black_box(42));
        });
    });

    let observed_events = (0..16).fold(
        EventBus::<u64>::new()
            .observed(BenchmarkEventObserver)
            .operation_id_generator(|| "benchmark-event".to_owned()),
        |events, index| events.context(format!("attribute-{index}"), format!("value-{index}")),
    );
    c.bench_function("observed event publish with 16 attributes", |b| {
        b.iter(|| {
            observed_events.publish_named(black_box("benchmark.event"), black_box(42));
        });
    });

    let unconfigured_jobs = ObservedJobRunner::new(BenchmarkJobObserver)
        .run_id_generator(|| "benchmark-job".to_owned());
    c.bench_function("observed job run with 0 attributes", |b| {
        b.iter(|| unconfigured_jobs.run(black_box(&BenchmarkJob)).unwrap());
    });

    let observed_jobs = (0..16).fold(
        ObservedJobRunner::new(BenchmarkJobObserver)
            .run_id_generator(|| "benchmark-job".to_owned()),
        |jobs, index| jobs.context(format!("attribute-{index}"), format!("value-{index}")),
    );
    c.bench_function("observed job run with 16 attributes", |b| {
        b.iter(|| observed_jobs.run(black_box(&BenchmarkJob)).unwrap());
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().noise_threshold(0.05);
    targets = integration_hot_paths
}
criterion_main!(benches);

# Jobs

`nidus-jobs` provides lightweight in-memory queues for synchronous and
Tokio-backed asynchronous background work.

```rust
struct SendDigest;

impl Job for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

let mut queue = JobQueue::new();
assert!(queue.is_empty());
queue.push(SendDigest);
assert_eq!(queue.len(), 1);

let report = queue.run_all();
assert!(report.is_success());
```

`run_all` executes jobs in insertion order and continues after failures. The
returned `JobReport` records completed job names and failed jobs with their
`JobError` details.

Use `AsyncJob` and `AsyncJobQueue` when a job awaits I/O or other Tokio tasks:

```rust
struct SendDigest;

#[async_trait::async_trait]
impl AsyncJob for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

let mut queue = AsyncJobQueue::new();
queue.push(SendDigest);

let report = queue.run_all().await;
assert!(report.is_success());
```

## Observed Jobs

`ObservedJobRunner` wraps individual `Job` and `AsyncJob` runs with operation
spans, generated run IDs, duration capture, status reporting, and context
attributes. It does not replace `JobQueue`; use it where workers execute jobs.

```rust
#[derive(Clone)]
struct MetricsObserver;

impl JobObserver for MetricsObserver {
    fn on_job_started(&self, context: &ObservedJobContext) {
        tracing::info!(job.name = context.job_name(), job.run_id = context.run_id());
    }

    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
        tracing::info!(
            job.name = context.job_name(),
            job.run_id = context.run_id(),
            ?status,
            duration_ms = context.duration().map(|duration| duration.as_millis())
        );
    }
}

let runner = ObservedJobRunner::new(MetricsObserver)
    .context("request_id", "req-123");
runner.run(&SendDigest)?;
```

Observers are replaceable, so applications can record Prometheus metrics, emit
events, or forward data to an external worker system without changing the job
trait.

For the recommended production path, pass `Observability::job_observer()`:

```rust
let observability = Observability::production("users-api").prometheus();
let runner = observability.job_runner();
runner.run(&SendDigest)?;
```

Only runs that go through `ObservedJobRunner` emit job metrics. Plain queue
execution stays available for applications that do not want instrumentation.

When observation needs slower export work, use a channel-backed observer:

```rust
let (observer, receiver) = job_observer_channel();
let runner = ObservedJobRunner::new(observer);

runner.run(&SendDigest)?;

for event in receiver.try_iter() {
    match event {
        ObservedJobEvent::Started(context) => {
            tracing::info!(job.name = context.job_name(), job.run_id = context.run_id());
        }
        ObservedJobEvent::Finished { context, status } => {
            tracing::info!(job.name = context.job_name(), ?status);
        }
    }
}
```

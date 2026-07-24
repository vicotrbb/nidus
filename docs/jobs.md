# Jobs and durable workflows

`nidus-jobs` preserves the existing lightweight `JobQueue`, `AsyncJobQueue`,
and `ObservedJobRunner` APIs for in-process work. It also provides a
backend-neutral durable job contract and bounded worker runtime. Install
`nidus-jobs-sqlx` when persisted delivery is required.

## In-process jobs

```rust
struct SendDigest;

impl nidus_jobs::Job for SendDigest {
    fn name(&self) -> &'static str { "send_digest" }
    fn run(&self) -> nidus_jobs::Result<()> { Ok(()) }
}

let mut queue = nidus_jobs::JobQueue::new();
queue.push(SendDigest);
assert!(queue.run_all().is_success());
```

In-process queues are intentionally non-durable. Existing public APIs and
observability wrappers remain source compatible.

## Durable jobs

The durable runtime consists of:

- `NewJob`, including schedule time, maximum attempts, correlation, and a
  backend-enforced idempotency key scoped to the handler name;
- `DurableJobStore`, whose lease and terminal mutations are atomic and compare
  both the current worker owner and attempt-generation fence;
- `DurableJobRegistry`, a typed set of named async handlers;
- `DurableJobWorker`, with bounded concurrency, lease heartbeats, exponential
  full-jitter retries, panic containment, crash recovery, cancellation, and a
  graceful drain deadline;
- `nidus-jobs-sqlx`, with SQLite, PostgreSQL, CockroachDB, and MySQL schemas,
  indexed ready/recovery/dead-letter paths, state counts, cancellation, and
  dead-letter inspection.

`nidus-jobs-sqlx` enables SQLite by default. Select `postgres`, `cockroach`, or
`mysql` for other stores; `health` adds `health_status` and
`register_ready_check`, `observability` adds the standard adapter observer, and
`dashboard` records redaction-safe adapter operations. `register` installs the
native store as a typed singleton, while its lifecycle hook migrates on startup
and closes the pool on shutdown.

PostgreSQL and CockroachDB require `sslmode=verify-full`, and MySQL requires
`ssl-mode=VERIFY_IDENTITY`. Plaintext is accepted only through a backend-
specific, loopback-only development opt-in.

```rust
use nidus_jobs::{DurableJobStore, NewJob};
use nidus_jobs_sqlx::{SqlxJobStore, SqlxJobStoreConfig};

let store = SqlxJobStore::connect(
    SqlxJobStoreConfig::postgres(std::env::var("DATABASE_URL")?)
        .with_max_connections(20)?,
).await?;
store.migrate().await?;
let result = store.enqueue(
    NewJob::new("email.welcome", serde_json::json!({"user_id": 42}))?
        .with_idempotency_key("welcome-user-42")?
        .with_max_attempts(8)?,
).await?;
```

Handlers return `JobExecutionError::retryable` or `permanent`. Persisted error
messages are capped and must already be stripped of secrets and PII. Retryable
failures are scheduled with bounded exponential full jitter. Exhausted,
permanent, panicking, and missing-handler executions move to `dead_lettered`.

### Handler identity and deployments

The stable handler name is part of each persisted record and of the
`(name, idempotency_key)` uniqueness contract. Workers resolve that string
through their immutable `DurableJobRegistry`; local queues and typed event
buses do not use this registry. Nidus does not persist process-local handler
indexes or derive durable identity from registration order.

Enqueue validates the name's syntax, not whether every active worker supports
it. A worker that leases an unknown name records a permanent missing-handler
failure and dead-letters the record. During mixed-version deployments, start
all workers that support a new name before producers enqueue it, and retain old
handlers until scheduled, retrying, and dead-letter retention windows have
closed. Nidus does not currently provide aliases, capability-aware leasing, or
first-party redrive.

## Delivery guarantee and idempotency

Durable jobs are at-least-once, never exactly-once. A process can perform a
side effect and crash before acknowledging the lease. Lease expiry then makes
the job eligible for another worker. Design handlers around an application
idempotency key, a database uniqueness constraint, compare-and-set state, or a
transactional outbox. The enqueue idempotency key prevents duplicate durable
records; it cannot make an arbitrary remote side effect exactly once.

## Scheduling, cancellation, and shutdown

`scheduled_at_ms` controls the first eligible lease time. Cancellation marks a
pending or running record and clears its lease; an executing handler also sees
a cancelled `CancellationToken` and must stop at cancellation-safe boundaries.
On shutdown, workers stop leasing immediately, cancel handler contexts, keep
heartbeating active leases during the configured grace window, and report any
tasks abandoned after the deadline.

The worker runs migrations and expired-lease recovery before polling. It also
repeats recovery while running, so multiple workers can safely compete for the
same database. The live suite proves concurrent lease exclusion on MySQL 8.4
and CockroachDB v26.2.0. SQLite deterministic tests cover retries, DLQs,
attempt-fenced recovery, cancellation, worker acknowledgement and graceful
drain, pre-migration failure, and file cleanup.

See the runnable `durable-jobs` binary in
`examples/integrations-production` and run all real-service checks with
`bash scripts/test-integration-services.sh`.

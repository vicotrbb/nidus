use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nidus_core::Container;
use nidus_jobs::{
    DurableJobHandler, DurableJobRegistry, DurableJobStore, DurableJobWorker, JobDisposition,
    JobExecutionContext, JobExecutionError, JobStatus, LeaseRequest, NewJob, WorkerConfig,
};
use nidus_jobs_sqlx::{SqlDialect, SqlxJobStore, SqlxJobStoreConfig};
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct TempDatabase {
    path: PathBuf,
}

impl TempDatabase {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("nidus-jobs-{}.sqlite", uuid::Uuid::new_v4())),
        }
    }

    fn url(&self) -> String {
        format!("sqlite://{}?mode=rwc", self.path.display())
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let candidate = PathBuf::from(format!("{}{suffix}", self.path.display()));
            if let Err(error) = std::fs::remove_file(candidate)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("failed to clean temporary SQLite database: {error}");
            }
        }
    }
}

async fn store(database: &TempDatabase) -> SqlxJobStore {
    SqlxJobStore::connect(
        SqlxJobStoreConfig::sqlite(database.url())
            .with_max_connections(4)
            .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn persists_idempotency_leases_retries_dlq_recovery_and_cancellation() {
    let database = TempDatabase::new();
    let store = store(&database).await;
    store.migrate().await.unwrap();
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'nidus_jobs'",
    )
    .fetch_all(store.pool())
    .await
    .unwrap();
    assert!(indexes.iter().any(|name| name == "nidus_jobs_ready_idx"));
    assert!(indexes.iter().any(|name| name == "nidus_jobs_recovery_idx"));
    assert!(
        indexes
            .iter()
            .any(|name| name == "nidus_jobs_dead_letter_idx")
    );

    let job = NewJob::new("email.send", json!({"user_id": 7}))
        .unwrap()
        .with_id("job-retry")
        .unwrap()
        .with_max_attempts(2)
        .unwrap()
        .with_idempotency_key("welcome-7")
        .unwrap()
        .scheduled_at_ms(0);
    assert!(store.enqueue(job.clone()).await.unwrap().was_enqueued());
    assert!(!store.enqueue(job).await.unwrap().was_enqueued());

    let first = store.clone();
    let second = store.clone();
    let (first_result, second_result) = tokio::join!(
        first.lease(LeaseRequest::new("worker-a", 1_000, Duration::from_secs(10), 1).unwrap()),
        second.lease(LeaseRequest::new("worker-b", 1_000, Duration::from_secs(10), 1).unwrap())
    );
    let first_result = first_result.unwrap();
    let second_result = second_result.unwrap();
    assert_eq!(first_result.len() + second_result.len(), 1);
    let leased = first_result
        .into_iter()
        .chain(second_result)
        .next()
        .unwrap();
    let owner = leased.lease_owner.as_deref().unwrap();
    assert_eq!(leased.attempts, 1);
    assert!(
        store
            .fail(
                &leased.id,
                owner,
                leased.attempts,
                "transient dependency failure",
                JobDisposition::RetryAt(1_001),
            )
            .await
            .unwrap()
    );

    let second_attempt = store
        .lease(LeaseRequest::new("worker-c", 1_001, Duration::from_secs(10), 1).unwrap())
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(second_attempt.attempts, 2);
    assert!(
        store
            .fail(
                &second_attempt.id,
                "worker-c",
                second_attempt.attempts,
                "permanent failure",
                JobDisposition::DeadLetter,
            )
            .await
            .unwrap()
    );
    let dead_letters = store.dead_letters(10).await.unwrap();
    assert_eq!(dead_letters.len(), 1);
    assert_eq!(dead_letters[0].status, JobStatus::DeadLettered);

    let crash_job = NewJob::new("crash.recover", json!({}))
        .unwrap()
        .with_id("job-crash")
        .unwrap()
        .scheduled_at_ms(0);
    store.enqueue(crash_job).await.unwrap();
    let crashed = store
        .lease(LeaseRequest::new("reused-worker", 2_000, Duration::from_millis(1), 1).unwrap())
        .await
        .unwrap();
    assert_eq!(crashed.len(), 1);
    assert_eq!(store.recover_expired_leases(2_002).await.unwrap(), 1);
    let recovered = store
        .lease(LeaseRequest::new("reused-worker", 2_002, Duration::from_secs(1), 1).unwrap())
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(
        !store
            .acknowledge(&recovered[0].id, "reused-worker", crashed[0].attempts)
            .await
            .unwrap(),
        "a stale attempt must not acknowledge a newer lease with the same worker id"
    );
    assert!(
        store
            .acknowledge(&recovered[0].id, "reused-worker", recovered[0].attempts,)
            .await
            .unwrap()
    );

    let cancelled = NewJob::new("cancel.me", json!({}))
        .unwrap()
        .with_id("job-cancel")
        .unwrap();
    store.enqueue(cancelled).await.unwrap();
    assert!(store.cancel("job-cancel").await.unwrap());

    let stats = store.health().await.unwrap();
    assert_eq!(stats.total(), 3);
    assert_eq!(stats.dead_lettered, 1);
    assert_eq!(stats.succeeded, 1);
    assert_eq!(stats.cancelled, 1);

    #[cfg(feature = "health")]
    {
        assert!(store.health_status().await.is_up());
        let _registry = Arc::new(store.clone())
            .register_ready_check(nidus_http::health::HealthRegistry::new(), "durable-jobs");
    }

    let mut container = Container::new();
    store.register(&mut container).unwrap();
    assert_eq!(
        container.resolve::<SqlxJobStore>().unwrap().dialect(),
        SqlDialect::Sqlite
    );

    store.close().await;
    let path = database.path().to_owned();
    drop(database);
    assert!(!path.exists());
}

struct CountingHandler(Arc<AtomicUsize>);

#[async_trait::async_trait]
impl DurableJobHandler for CountingHandler {
    fn name(&self) -> &'static str {
        "count"
    }

    async fn execute(&self, _context: JobExecutionContext) -> Result<(), JobExecutionError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn worker_acknowledges_and_drains_on_shutdown() {
    let database = TempDatabase::new();
    let store = store(&database).await;
    store.migrate().await.unwrap();
    store
        .enqueue(NewJob::new("count", json!({})).unwrap())
        .await
        .unwrap();

    let executions = Arc::new(AtomicUsize::new(0));
    let mut registry = DurableJobRegistry::new();
    registry
        .register(CountingHandler(Arc::clone(&executions)))
        .unwrap();
    let config = WorkerConfig::new()
        .with_worker_id("test-worker")
        .unwrap()
        .with_concurrency(1)
        .unwrap()
        .with_lease_batch_size(1)
        .unwrap()
        .with_lease_timing(Duration::from_millis(300), Duration::from_millis(100))
        .unwrap()
        .with_poll_interval(Duration::from_millis(5))
        .unwrap()
        .with_shutdown_grace(Duration::from_secs(1));
    let shutdown = CancellationToken::new();
    let worker = DurableJobWorker::new(Arc::new(store.clone()), registry, config);
    let task = tokio::spawn(worker.run(shutdown.clone()));

    tokio::time::timeout(Duration::from_secs(2), async {
        while executions.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    shutdown.cancel();
    let report = task.await.unwrap().unwrap();
    assert_eq!(report.succeeded, 1);
    assert_eq!(report.abandoned, 0);
    assert_eq!(store.stats().await.unwrap().succeeded, 1);

    store.close().await;
}

#[tokio::test]
async fn reports_failure_before_schema_migration_without_leaking_files() {
    let database = TempDatabase::new();
    let store = store(&database).await;
    let error = store
        .enqueue(NewJob::new("not.migrated", json!({})).unwrap())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("store operation failed"));
    store.close().await;

    let path = database.path().to_owned();
    drop(database);
    assert!(!path.exists());
}

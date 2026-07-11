use std::{sync::Arc, time::Duration};

use nidus_jobs::{
    DurableJobHandler, DurableJobRegistry, DurableJobStore, DurableJobWorker, JobExecutionContext,
    JobExecutionError, NewJob, WorkerConfig,
};
use nidus_jobs_sqlx::{SqlxJobStore, SqlxJobStoreConfig};
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct PrintJob;

#[async_trait::async_trait]
impl DurableJobHandler for PrintJob {
    fn name(&self) -> &'static str {
        "example.print"
    }

    async fn execute(&self, context: JobExecutionContext) -> Result<(), JobExecutionError> {
        println!("executing durable job {}", context.job().id);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store = SqlxJobStore::connect(
        SqlxJobStoreConfig::sqlite("sqlite::memory:").with_max_connections(1)?,
    )
    .await?;
    store.migrate().await?;
    store
        .enqueue(
            NewJob::new("example.print", json!({"message": "hello"}))?
                .with_idempotency_key("example-once")?,
        )
        .await?;

    let mut registry = DurableJobRegistry::new();
    registry.register(PrintJob)?;
    let config = WorkerConfig::new()
        .with_worker_id("example-worker")?
        .with_concurrency(1)?
        .with_lease_batch_size(1)?
        .with_poll_interval(Duration::from_millis(10))?;
    let shutdown = CancellationToken::new();
    let task = tokio::spawn(
        DurableJobWorker::new(Arc::new(store.clone()), registry, config).run(shutdown.clone()),
    );

    let completed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if store.stats().await?.succeeded == 1 {
                return Ok::<(), nidus_jobs::DurableJobError>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    shutdown.cancel();
    let report = task.await??;
    completed??;
    println!("worker report: {report:?}");
    store.close().await;
    Ok(())
}

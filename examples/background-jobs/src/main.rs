//! Background job queue example using synchronous and asynchronous Nidus jobs.

use nidus_jobs::{AsyncJob, AsyncJobQueue, Job, JobError, JobQueue, JobReport};

struct SendDigest;
struct RebuildSearchProjection;
struct SyncSearchIndex;
struct RefreshMaterializedView;

impl Job for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        println!("digest sent");
        Ok(())
    }
}

impl Job for RebuildSearchProjection {
    fn name(&self) -> &'static str {
        "rebuild_search_projection"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Err(JobError::new("search projection source is unavailable"))
    }
}

#[async_trait::async_trait]
impl AsyncJob for SyncSearchIndex {
    fn name(&self) -> &'static str {
        "sync_search_index"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        println!("search index synced");
        Ok(())
    }
}

#[async_trait::async_trait]
impl AsyncJob for RefreshMaterializedView {
    fn name(&self) -> &'static str {
        "refresh_materialized_view"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        Err(JobError::new("materialized view refresh timed out"))
    }
}

fn run_sync_jobs() -> JobReport {
    let mut queue = JobQueue::new();
    queue.push(SendDigest);
    queue.push(RebuildSearchProjection);
    queue.run_all()
}

async fn run_async_jobs() -> JobReport {
    let mut queue = AsyncJobQueue::new();
    queue.push(SyncSearchIndex);
    queue.push(RefreshMaterializedView);
    queue.run_all().await
}

#[tokio::main]
async fn main() {
    let report = run_sync_jobs();
    println!("completed jobs: {:?}", report.completed());
    println!("failed jobs: {:?}", report.failed());

    let report = run_async_jobs().await;
    println!("completed async jobs: {:?}", report.completed());
    println!("failed async jobs: {:?}", report.failed());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_jobs_report_completed_and_failed_work() {
        let report = run_sync_jobs();

        assert_eq!(report.completed(), ["send_digest"]);
        assert_eq!(report.failed().len(), 1);
        assert_eq!(report.failed()[0].job(), "rebuild_search_projection");
        assert_eq!(
            report.failed()[0].error().message(),
            "search projection source is unavailable"
        );
        assert!(!report.is_success());
    }

    #[tokio::test]
    async fn async_jobs_report_completed_and_failed_work() {
        let report = run_async_jobs().await;

        assert_eq!(report.completed(), ["sync_search_index"]);
        assert_eq!(report.failed().len(), 1);
        assert_eq!(report.failed()[0].job(), "refresh_materialized_view");
        assert_eq!(
            report.failed()[0].error().message(),
            "materialized view refresh timed out"
        );
        assert!(!report.is_success());
    }
}

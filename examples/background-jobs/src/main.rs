use nidus_jobs::{AsyncJob, AsyncJobQueue, Job, JobQueue};

struct SendDigest;
struct SyncSearchIndex;

impl Job for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        println!("digest sent");
        Ok(())
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

#[tokio::main]
async fn main() {
    let mut queue = JobQueue::new();
    queue.push(SendDigest);
    let report = queue.run_all();
    println!("completed jobs: {:?}", report.completed());
    println!("failed jobs: {:?}", report.failed());

    let mut async_queue = AsyncJobQueue::new();
    async_queue.push(SyncSearchIndex);
    let report = async_queue.run_all().await;
    println!("completed async jobs: {:?}", report.completed());
    println!("failed async jobs: {:?}", report.failed());
}

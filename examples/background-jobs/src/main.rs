use nidus_jobs::{Job, JobQueue};

struct SendDigest;

impl Job for SendDigest {
    fn name(&self) -> &'static str {
        "send_digest"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        println!("digest sent");
        Ok(())
    }
}

fn main() {
    let mut queue = JobQueue::new();
    queue.push(SendDigest);
    let report = queue.run_all();
    println!("completed jobs: {:?}", report.completed());
    println!("failed jobs: {:?}", report.failed());
}

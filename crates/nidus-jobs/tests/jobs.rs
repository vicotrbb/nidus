use std::sync::{Arc, Mutex};

use nidus_jobs::{Job, JobQueue};

#[derive(Clone)]
struct RecordJob(Arc<Mutex<Vec<&'static str>>>);

impl Job for RecordJob {
    fn name(&self) -> &'static str {
        "record"
    }

    fn run(&self) {
        self.0.lock().unwrap().push("ran");
    }
}

#[test]
fn job_queue_runs_registered_jobs_in_order() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = JobQueue::new();
    queue.push(RecordJob(Arc::clone(&records)));

    let report = queue.run_all();

    assert_eq!(report.completed(), ["record"]);
    assert_eq!(*records.lock().unwrap(), ["ran"]);
}

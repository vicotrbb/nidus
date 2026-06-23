use std::sync::{Arc, Mutex};

use nidus_jobs::{Job, JobError, JobQueue};

#[derive(Clone)]
struct RecordJob(Arc<Mutex<Vec<&'static str>>>);
struct FailingJob;

impl Job for RecordJob {
    fn name(&self) -> &'static str {
        "record"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        self.0.lock().unwrap().push("ran");
        Ok(())
    }
}

impl Job for FailingJob {
    fn name(&self) -> &'static str {
        "fail"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Err(JobError::new("job failed"))
    }
}

#[test]
fn job_queue_runs_registered_jobs_in_order() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = JobQueue::new();
    queue.push(RecordJob(Arc::clone(&records)));

    let report = queue.run_all();

    assert_eq!(report.completed(), ["record"]);
    assert!(report.failed().is_empty());
    assert!(report.is_success());
    assert_eq!(*records.lock().unwrap(), ["ran"]);
}

#[test]
fn job_queue_reports_failures_and_continues_running() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = JobQueue::new();
    queue.push(FailingJob);
    queue.push(RecordJob(Arc::clone(&records)));

    let report = queue.run_all();

    assert_eq!(report.completed(), ["record"]);
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].job(), "fail");
    assert_eq!(report.failed()[0].error().message(), "job failed");
    assert!(!report.is_success());
    assert_eq!(*records.lock().unwrap(), ["ran"]);
}

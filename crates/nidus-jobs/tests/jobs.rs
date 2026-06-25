use std::sync::{Arc, Mutex};

use nidus_jobs::{AsyncJob, AsyncJobQueue, Job, JobError, JobQueue};

#[derive(Clone)]
struct RecordJob(Arc<Mutex<Vec<&'static str>>>);
#[derive(Clone)]
struct AsyncRecordJob(Arc<Mutex<Vec<&'static str>>>);
struct FailingJob;
struct AsyncFailingJob;
struct PanickingJob;

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

impl Job for PanickingJob {
    fn name(&self) -> &'static str {
        "panic"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        panic!("job panic")
    }
}

#[async_trait::async_trait]
impl AsyncJob for AsyncRecordJob {
    fn name(&self) -> &'static str {
        "async_record"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        self.0.lock().unwrap().push("async_ran");
        Ok(())
    }
}

#[async_trait::async_trait]
impl AsyncJob for AsyncFailingJob {
    fn name(&self) -> &'static str {
        "async_fail"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        Err(JobError::new("async job failed"))
    }
}

#[test]
fn job_queue_runs_registered_jobs_in_order() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = JobQueue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);

    queue.push(RecordJob(Arc::clone(&records)));
    assert!(!queue.is_empty());
    assert_eq!(queue.len(), 1);

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

#[test]
fn job_queue_reports_panics_and_continues_running() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = JobQueue::new();
    queue.push(PanickingJob);
    queue.push(RecordJob(Arc::clone(&records)));

    let report = queue.run_all();

    assert_eq!(report.completed(), ["record"]);
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].job(), "panic");
    assert_eq!(report.failed()[0].error().message(), "job panicked");
    assert!(!report.is_success());
    assert_eq!(*records.lock().unwrap(), ["ran"]);
}

#[tokio::test]
async fn async_job_queue_runs_registered_jobs_in_order() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = AsyncJobQueue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);

    queue.push(AsyncRecordJob(Arc::clone(&records)));
    assert!(!queue.is_empty());
    assert_eq!(queue.len(), 1);

    let report = queue.run_all().await;

    assert_eq!(report.completed(), ["async_record"]);
    assert!(report.failed().is_empty());
    assert!(report.is_success());
    assert_eq!(*records.lock().unwrap(), ["async_ran"]);
}

#[tokio::test]
async fn async_job_queue_reports_failures_and_continues_running() {
    let records = Arc::new(Mutex::new(Vec::new()));
    let mut queue = AsyncJobQueue::new();
    queue.push(AsyncFailingJob);
    queue.push(AsyncRecordJob(Arc::clone(&records)));

    let report = queue.run_all().await;

    assert_eq!(report.completed(), ["async_record"]);
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].job(), "async_fail");
    assert_eq!(report.failed()[0].error().message(), "async job failed");
    assert!(!report.is_success());
    assert_eq!(*records.lock().unwrap(), ["async_ran"]);
}

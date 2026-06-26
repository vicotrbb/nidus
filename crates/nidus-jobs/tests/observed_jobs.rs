use std::sync::{Arc, Mutex};

use nidus_jobs::{
    AsyncJob, Job, JobError, JobObserver, JobResultStatus, ObservedJobContext, ObservedJobRunner,
};

#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingObserver {
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl JobObserver for RecordingObserver {
    fn on_job_started(&self, context: &ObservedJobContext) {
        self.events.lock().unwrap().push(format!(
            "started {} {}",
            context.job_name(),
            context.run_id()
        ));
    }

    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
        self.events.lock().unwrap().push(format!(
            "finished {} {} {:?}",
            context.job_name(),
            context.run_id(),
            status
        ));
    }
}

struct SuccessfulJob;

impl Job for SuccessfulJob {
    fn name(&self) -> &'static str {
        "successful_job"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

struct FailingJob;

impl Job for FailingJob {
    fn name(&self) -> &'static str {
        "failing_job"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        Err(JobError::new("job failed"))
    }
}

struct AsyncSuccessfulJob;

#[async_trait::async_trait]
impl AsyncJob for AsyncSuccessfulJob {
    fn name(&self) -> &'static str {
        "async_successful_job"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        Ok(())
    }
}

struct PanickingJob;

impl Job for PanickingJob {
    fn name(&self) -> &'static str {
        "panicking_job"
    }

    fn run(&self) -> nidus_jobs::Result<()> {
        panic!("sync job panicked");
    }
}

struct AsyncPanickingJob;

#[async_trait::async_trait]
impl AsyncJob for AsyncPanickingJob {
    fn name(&self) -> &'static str {
        "async_panicking_job"
    }

    async fn run(&self) -> nidus_jobs::Result<()> {
        panic!("async job panicked");
    }
}

#[test]
fn observed_job_runner_emits_run_ids_status_and_context() {
    let observer = RecordingObserver::default();
    let runner = ObservedJobRunner::new(observer.clone())
        .context("request_id", "req-123")
        .run_id_generator(|| "run-1".to_owned());

    runner.run(&SuccessfulJob).unwrap();
    let error = runner.run(&FailingJob).unwrap_err();

    assert_eq!(error.message(), "job failed");
    assert_eq!(
        observer.events(),
        [
            "started successful_job run-1",
            "finished successful_job run-1 Success",
            "started failing_job run-1",
            "finished failing_job run-1 Failure"
        ]
    );
}

#[tokio::test]
async fn observed_job_runner_supports_async_jobs() {
    let observer = RecordingObserver::default();
    let runner = ObservedJobRunner::new(observer.clone()).run_id_generator(|| "run-2".to_owned());

    runner.run_async(&AsyncSuccessfulJob).await.unwrap();

    assert_eq!(
        observer.events(),
        [
            "started async_successful_job run-2",
            "finished async_successful_job run-2 Success"
        ]
    );
}

#[test]
fn observed_job_runner_emits_finished_and_returns_error_after_sync_panic() {
    let observer = RecordingObserver::default();
    let runner = ObservedJobRunner::new(observer.clone()).run_id_generator(|| "run-3".to_owned());

    let error = runner.run(&PanickingJob).unwrap_err();

    assert_eq!(error.message(), "job panicked");
    assert_eq!(
        observer.events(),
        [
            "started panicking_job run-3",
            "finished panicking_job run-3 Failure"
        ]
    );
}

#[tokio::test]
async fn observed_job_runner_emits_finished_and_returns_error_after_async_panic() {
    let observer = RecordingObserver::default();
    let runner = ObservedJobRunner::new(observer.clone()).run_id_generator(|| "run-4".to_owned());

    let error = runner.run_async(&AsyncPanickingJob).await.unwrap_err();

    assert_eq!(error.message(), "job panicked");
    assert_eq!(
        observer.events(),
        [
            "started async_panicking_job run-4",
            "finished async_panicking_job run-4 Failure"
        ]
    );
}

use std::sync::{Arc, Mutex};

use nidus_jobs::{
    AsyncJob, AsyncJobQueue, Job, JobError, JobObserver, JobObserverChannel, JobQueue,
    JobResultStatus, ObservedJobContext, ObservedJobEvent, ObservedJobRunner, job_observer_channel,
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
fn observed_job_runner_async_future_is_send() {
    fn assert_send<T: Send>(_value: T) {}

    let runner = ObservedJobRunner::new(());
    let job = AsyncSuccessfulJob;

    assert_send(runner.run_async(&job));
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

#[test]
fn job_queue_can_run_all_jobs_with_observer() {
    let observer = RecordingObserver::default();
    let runner =
        ObservedJobRunner::new(observer.clone()).run_id_generator(|| "queue-run".to_owned());
    let mut queue = JobQueue::new();
    queue.push(SuccessfulJob);
    queue.push(FailingJob);

    let report = queue.run_all_observed(&runner);

    assert_eq!(report.completed(), ["successful_job"]);
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].job(), "failing_job");
    assert_eq!(
        observer.events(),
        [
            "started successful_job queue-run",
            "finished successful_job queue-run Success",
            "started failing_job queue-run",
            "finished failing_job queue-run Failure"
        ]
    );
}

#[tokio::test]
async fn async_job_queue_can_run_all_jobs_with_observer() {
    let observer = RecordingObserver::default();
    let runner =
        ObservedJobRunner::new(observer.clone()).run_id_generator(|| "queue-run".to_owned());
    let mut queue = AsyncJobQueue::new();
    queue.push(AsyncSuccessfulJob);
    queue.push(AsyncPanickingJob);

    let report = queue.run_all_observed(&runner).await;

    assert_eq!(report.completed(), ["async_successful_job"]);
    assert_eq!(report.failed().len(), 1);
    assert_eq!(report.failed()[0].job(), "async_panicking_job");
    assert_eq!(report.failed()[0].error().message(), "job panicked");
    assert_eq!(
        observer.events(),
        [
            "started async_successful_job queue-run",
            "finished async_successful_job queue-run Success",
            "started async_panicking_job queue-run",
            "finished async_panicking_job queue-run Failure"
        ]
    );
}

#[test]
fn job_observer_channel_emits_structured_started_and_finished_events() {
    let (observer, receiver) = job_observer_channel();
    let _: JobObserverChannel = observer.clone();
    let runner = ObservedJobRunner::new(observer).run_id_generator(|| "channel-run".to_owned());

    runner.run(&SuccessfulJob).unwrap();

    match receiver.try_recv().unwrap() {
        ObservedJobEvent::Started(context) => {
            assert_eq!(context.job_name(), "successful_job");
            assert_eq!(context.run_id(), "channel-run");
            assert_eq!(context.duration(), None);
        }
        event => panic!("expected started event, got {event:?}"),
    }
    match receiver.try_recv().unwrap() {
        ObservedJobEvent::Finished { context, status } => {
            assert_eq!(context.job_name(), "successful_job");
            assert_eq!(context.run_id(), "channel-run");
            assert_eq!(status, JobResultStatus::Success);
            assert!(context.duration().is_some());
        }
        event => panic!("expected finished event, got {event:?}"),
    }
    assert!(receiver.try_recv().is_err());
}

#[test]
fn job_observer_channel_emits_failure_events_for_job_errors() {
    let (observer, receiver) = job_observer_channel();
    let runner = ObservedJobRunner::new(observer).run_id_generator(|| "channel-error".to_owned());

    let error = runner.run(&FailingJob).unwrap_err();

    assert_eq!(error.message(), "job failed");
    let _started = receiver.try_recv().unwrap();
    match receiver.try_recv().unwrap() {
        ObservedJobEvent::Finished { context, status } => {
            assert_eq!(context.job_name(), "failing_job");
            assert_eq!(context.run_id(), "channel-error");
            assert_eq!(status, JobResultStatus::Failure);
        }
        event => panic!("expected failure event, got {event:?}"),
    }
}

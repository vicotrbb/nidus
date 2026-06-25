#![deny(missing_docs)]

//! Background job abstractions.
//!
//! These primitives are intentionally local and in-memory. `JobQueue` and
//! `AsyncJobQueue` run jobs stored in the current process; they do not persist,
//! schedule, retry, distribute, or reserve jobs across workers.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::{Duration, Instant},
};

/// Synchronous job abstraction for lightweight background work.
///
/// Implement this for short local jobs that can run on the calling thread. Use
/// [`AsyncJob`] for Tokio-backed work.
pub trait Job: Send + Sync + 'static {
    /// Stable job name.
    fn name(&self) -> &'static str;

    /// Runs the job.
    fn run(&self) -> Result<()>;
}

/// Asynchronous job abstraction for Tokio-backed background work.
///
/// Implement this for jobs that need `.await`. The built-in async queue still
/// runs jobs sequentially in the current process.
#[async_trait::async_trait]
pub trait AsyncJob: Send + Sync + 'static {
    /// Stable job name.
    fn name(&self) -> &'static str;

    /// Runs the job asynchronously.
    async fn run(&self) -> Result<()>;
}

/// Completion status for an observed job run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobResultStatus {
    /// Job completed successfully.
    Success,
    /// Job returned an error.
    Failure,
}

/// Context carried through observed job execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedJobContext {
    run_id: String,
    job_name: &'static str,
    attributes: BTreeMap<String, String>,
    duration: Option<Duration>,
}

impl ObservedJobContext {
    /// Creates context for a job run.
    pub fn new(run_id: impl Into<String>, job_name: &'static str) -> Self {
        Self {
            run_id: run_id.into(),
            job_name,
            attributes: BTreeMap::new(),
            duration: None,
        }
    }

    /// Adds an attribute to the job context.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Sets the observed duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Returns the run id.
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Returns the stable job name.
    pub fn job_name(&self) -> &'static str {
        self.job_name
    }

    /// Returns context attributes.
    pub fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    /// Returns the observed job duration when the job has finished.
    pub const fn duration(&self) -> Option<Duration> {
        self.duration
    }
}

/// Observer hook for job execution.
///
/// Hooks run synchronously around the job execution path. Keep them lightweight
/// or forward to your own telemetry/export queue.
pub trait JobObserver: Clone + Send + Sync + 'static {
    /// Called immediately before a job is run.
    fn on_job_started(&self, context: &ObservedJobContext);

    /// Called after a job finishes or fails.
    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus);
}

impl JobObserver for () {
    fn on_job_started(&self, _context: &ObservedJobContext) {}

    fn on_job_finished(&self, _context: &ObservedJobContext, _status: JobResultStatus) {}
}

/// Runner that observes synchronous and asynchronous jobs without owning a queue.
///
/// The runner creates a tracing span and calls a [`JobObserver`] before and
/// after a single job run. It does not enqueue, retry, or schedule jobs.
///
/// ```ignore
/// use nidus_jobs::{
///     Job, JobObserver, JobResultStatus, ObservedJobContext, ObservedJobRunner,
/// };
///
/// struct ReindexUsers;
///
/// impl Job for ReindexUsers {
///     fn name(&self) -> &'static str { "reindex_users" }
///     fn run(&self) -> nidus_jobs::Result<()> { Ok(()) }
/// }
///
/// #[derive(Clone)]
/// struct Observer;
///
/// impl JobObserver for Observer {
///     fn on_job_started(&self, context: &ObservedJobContext) {
///         tracing::info!(job = context.job_name(), run_id = context.run_id());
///     }
///
///     fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
///         tracing::info!(job = context.job_name(), ?status);
///     }
/// }
///
/// ObservedJobRunner::new(Observer)
///     .context("service", "users-api")
///     .run(&ReindexUsers)?;
/// # Ok::<(), nidus_jobs::JobError>(())
/// ```
#[derive(Clone)]
pub struct ObservedJobRunner<O = ()> {
    observer: O,
    attributes: BTreeMap<String, String>,
    run_id_generator: Arc<dyn Fn() -> String + Send + Sync>,
}

impl<O> ObservedJobRunner<O>
where
    O: JobObserver,
{
    /// Creates an observed job runner.
    pub fn new(observer: O) -> Self {
        Self {
            observer,
            attributes: BTreeMap::new(),
            run_id_generator: Arc::new(|| uuid::Uuid::new_v4().to_string()),
        }
    }

    /// Adds a context attribute propagated to every observed job.
    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Replaces the run id generator.
    pub fn run_id_generator(
        mut self,
        generator: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        self.run_id_generator = Arc::new(generator);
        self
    }

    /// Runs and observes a synchronous job.
    pub fn run<J>(&self, job: &J) -> Result<()>
    where
        J: Job,
    {
        let started_at = Instant::now();
        let mut context = self.context_for(job.name());
        let span = tracing::info_span!(
            "job.run",
            job.name = job.name(),
            job.run_id = context.run_id()
        );
        let _entered = span.enter();
        self.observer.on_job_started(&context);
        let result = job.run();
        context = context.with_duration(started_at.elapsed());
        self.observer
            .on_job_finished(&context, status_for_result(&result));
        result
    }

    /// Runs and observes an asynchronous job.
    pub async fn run_async<J>(&self, job: &J) -> Result<()>
    where
        J: AsyncJob,
    {
        let started_at = Instant::now();
        let mut context = self.context_for(job.name());
        let span = tracing::info_span!(
            "job.run",
            job.name = job.name(),
            job.run_id = context.run_id()
        );
        let _entered = span.enter();
        self.observer.on_job_started(&context);
        let result = job.run().await;
        context = context.with_duration(started_at.elapsed());
        self.observer
            .on_job_finished(&context, status_for_result(&result));
        result
    }

    fn context_for(&self, job_name: &'static str) -> ObservedJobContext {
        let mut context = ObservedJobContext::new((self.run_id_generator)(), job_name);
        for (key, value) in &self.attributes {
            context = context.with_attribute(key.clone(), value.clone());
        }
        context
    }
}

fn status_for_result<T>(result: &std::result::Result<T, JobError>) -> JobResultStatus {
    if result.is_ok() {
        JobResultStatus::Success
    } else {
        JobResultStatus::Failure
    }
}

/// Result type for background job execution.
pub type Result<T> = std::result::Result<T, JobError>;

/// Error emitted by a background job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobError {
    message: String,
}

impl JobError {
    /// Creates a job error with a human-readable message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for JobError {}

/// In-memory job queue.
///
/// Jobs are retained after [`Self::run_all`], so calling `run_all` again runs
/// the same jobs again. Execution is sequential and in insertion order.
///
/// ```ignore
/// use nidus_jobs::{Job, JobQueue};
///
/// struct SendDigest;
///
/// impl Job for SendDigest {
///     fn name(&self) -> &'static str { "send_digest" }
///     fn run(&self) -> nidus_jobs::Result<()> { Ok(()) }
/// }
///
/// let mut queue = JobQueue::new();
/// queue.push(SendDigest);
///
/// let report = queue.run_all();
/// assert!(report.is_success());
/// assert_eq!(report.completed(), &["send_digest"]);
/// ```
#[derive(Default)]
pub struct JobQueue {
    jobs: Vec<Box<dyn Job>>,
}

impl JobQueue {
    /// Creates an empty job queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes a job into the queue.
    ///
    /// The job is boxed and kept in memory until the queue is dropped.
    pub fn push<J>(&mut self, job: J)
    where
        J: Job,
    {
        self.jobs.push(Box::new(job));
    }

    /// Returns the number of queued jobs.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Returns whether the queue has no jobs.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Runs all queued jobs in insertion order.
    ///
    /// Every job is attempted. Failures are collected in the returned
    /// [`JobReport`] and do not stop later jobs from running.
    pub fn run_all(&self) -> JobReport {
        let mut completed = Vec::with_capacity(self.jobs.len());
        let mut failed = Vec::new();
        for job in &self.jobs {
            match catch_unwind(AssertUnwindSafe(|| job.run())) {
                Ok(Ok(())) => completed.push(job.name()),
                Ok(Err(error)) => failed.push(JobFailure {
                    job: job.name(),
                    error,
                }),
                Err(_) => failed.push(JobFailure {
                    job: job.name(),
                    error: JobError::new("job panicked"),
                }),
            }
        }
        JobReport { completed, failed }
    }
}

/// In-memory asynchronous job queue.
///
/// Jobs are retained after [`Self::run_all`], so calling `run_all` again awaits
/// the same jobs again. Execution is sequential and in insertion order on the
/// current Tokio task.
#[derive(Default)]
pub struct AsyncJobQueue {
    jobs: Vec<Box<dyn AsyncJob>>,
}

impl AsyncJobQueue {
    /// Creates an empty asynchronous job queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes an asynchronous job into the queue.
    pub fn push<J>(&mut self, job: J)
    where
        J: AsyncJob,
    {
        self.jobs.push(Box::new(job));
    }

    /// Returns the number of queued asynchronous jobs.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Returns whether the queue has no asynchronous jobs.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Runs all queued asynchronous jobs in insertion order.
    ///
    /// Every job is attempted. Failures are collected in the returned
    /// [`JobReport`] and do not stop later jobs from running.
    pub async fn run_all(&self) -> JobReport {
        let mut completed = Vec::with_capacity(self.jobs.len());
        let mut failed = Vec::new();
        for job in &self.jobs {
            match job.run().await {
                Ok(()) => completed.push(job.name()),
                Err(error) => failed.push(JobFailure {
                    job: job.name(),
                    error,
                }),
            }
        }
        JobReport { completed, failed }
    }
}

/// Report from a queue run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobReport {
    completed: Vec<&'static str>,
    failed: Vec<JobFailure>,
}

impl JobReport {
    /// Returns completed job names in execution order.
    pub fn completed(&self) -> &[&'static str] {
        &self.completed
    }

    /// Returns failed jobs in execution order.
    pub fn failed(&self) -> &[JobFailure] {
        &self.failed
    }

    /// Returns whether every queued job completed successfully.
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Failed job details from a queue run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobFailure {
    job: &'static str,
    error: JobError,
}

impl JobFailure {
    /// Returns the failed job name.
    pub fn job(&self) -> &'static str {
        self.job
    }

    /// Returns the job error.
    pub fn error(&self) -> &JobError {
        &self.error
    }
}

//! Background job abstractions.

use std::{error::Error, fmt};

/// Synchronous job abstraction for lightweight background work.
pub trait Job: Send + Sync + 'static {
    /// Stable job name.
    fn name(&self) -> &'static str;

    /// Runs the job.
    fn run(&self) -> Result<()>;
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
    pub fn push<J>(&mut self, job: J)
    where
        J: Job,
    {
        self.jobs.push(Box::new(job));
    }

    /// Runs all queued jobs in insertion order.
    pub fn run_all(&self) -> JobReport {
        let mut completed = Vec::with_capacity(self.jobs.len());
        let mut failed = Vec::new();
        for job in &self.jobs {
            match job.run() {
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

//! Background job abstractions.

/// Synchronous job abstraction for lightweight background work.
pub trait Job: Send + Sync + 'static {
    /// Stable job name.
    fn name(&self) -> &'static str;

    /// Runs the job.
    fn run(&self);
}

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
        for job in &self.jobs {
            job.run();
            completed.push(job.name());
        }
        JobReport { completed }
    }
}

/// Report from a queue run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobReport {
    completed: Vec<&'static str>,
}

impl JobReport {
    /// Returns completed job names in execution order.
    pub fn completed(&self) -> &[&'static str] {
        &self.completed
    }
}

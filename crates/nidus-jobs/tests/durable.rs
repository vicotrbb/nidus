use std::time::Duration;

use nidus_jobs::{
    DurableJobHandler, DurableJobRegistry, JobExecutionContext, JobExecutionError, JobRetryPolicy,
    NewJob, WorkerConfig,
};
use serde_json::json;

struct TestHandler;

#[async_trait::async_trait]
impl DurableJobHandler for TestHandler {
    fn name(&self) -> &'static str {
        "test.handler"
    }

    async fn execute(&self, _context: JobExecutionContext) -> Result<(), JobExecutionError> {
        Ok(())
    }
}

#[test]
fn validates_job_boundaries_and_idempotency_metadata() {
    let job = NewJob::new("email.send", json!({"user_id": 42}))
        .unwrap()
        .with_id("job-42")
        .unwrap()
        .with_max_attempts(7)
        .unwrap()
        .with_idempotency_key("welcome-42")
        .unwrap()
        .with_correlation_id("request-9")
        .unwrap()
        .scheduled_at_ms(1234);

    assert_eq!(job.id(), "job-42");
    assert_eq!(job.name(), "email.send");
    assert_eq!(job.max_attempts(), 7);
    assert_eq!(job.available_at_ms(), 1234);
    assert_eq!(job.idempotency_key(), Some("welcome-42"));
    assert_eq!(job.correlation_id(), Some("request-9"));
    let debug = format!("{job:?}");
    assert!(!debug.contains("user_id"));
    assert!(!debug.contains("welcome-42"));
    assert!(debug.contains("<redacted>"));

    assert!(NewJob::new("", json!({})).is_err());
    assert!(NewJob::new("exact-limit", json!("x".repeat(1024 * 1024 - 2))).is_ok());
    assert!(NewJob::new("over-limit", json!("x".repeat(1024 * 1024 - 1))).is_err());
    assert!(NewJob::new("oversized", json!("x".repeat(1024 * 1024 + 1))).is_err());
    assert!(
        NewJob::new("attempts", json!({}))
            .unwrap()
            .with_max_attempts(0)
            .is_err()
    );
}

#[test]
fn rejects_duplicate_handler_names() {
    let mut registry = DurableJobRegistry::new();
    registry.register(TestHandler).unwrap();

    assert!(registry.contains("test.handler"));
    assert_eq!(registry.len(), 1);
    assert!(registry.register(TestHandler).is_err());
}

#[test]
fn retry_policy_is_bounded_and_worker_limits_are_validated() {
    let policy = JobRetryPolicy::new(Duration::from_millis(100), Duration::from_secs(1)).unwrap();
    assert_eq!(policy.maximum_delay(1), Duration::from_millis(100));
    assert_eq!(policy.maximum_delay(5), Duration::from_secs(1));
    assert!(policy.delay(5) <= Duration::from_secs(1));

    assert!(WorkerConfig::new().with_concurrency(0).is_err());
    assert!(WorkerConfig::new().with_lease_batch_size(0).is_err());
    assert!(
        WorkerConfig::new()
            .with_lease_timing(Duration::from_secs(1), Duration::from_secs(1))
            .is_err()
    );
}

#[test]
fn persisted_handler_errors_are_utf8_safe_and_bounded() {
    let error = JobExecutionError::retryable("🪺".repeat(1_000));
    assert!(error.message().len() <= 2_048);
    assert!(error.is_retryable());
}

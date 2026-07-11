//! Backend-neutral durable job contracts and worker runtime.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt, io,
    panic::AssertUnwindSafe,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{task::JoinSet, time::Instant};
use tokio_util::sync::CancellationToken;

const MAX_JOB_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 255;
const MAX_ERROR_BYTES: usize = 2_048;
const MAX_WORKER_CONCURRENCY: usize = 1_024;

/// Result type used by durable job stores and workers.
pub type DurableResult<T> = std::result::Result<T, DurableJobError>;

/// Error returned by durable job infrastructure.
#[derive(Debug, thiserror::Error)]
pub enum DurableJobError {
    /// A configuration value or durable record is invalid.
    #[error("invalid durable job configuration: {0}")]
    Configuration(String),
    /// A job payload could not be encoded or decoded.
    #[error("durable job serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The persistence backend returned an error.
    #[error("durable job store operation failed: {message}")]
    Store {
        /// Safe, non-secret operation context.
        message: String,
        /// Original backend error.
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },
    /// A worker task could not be joined.
    #[error("durable job task failed: {0}")]
    TaskJoin(String),
}

impl DurableJobError {
    /// Wraps a backend error with safe operation context.
    pub fn store(message: impl Into<String>, source: impl Error + Send + Sync + 'static) -> Self {
        Self::Store {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Creates a store error without attaching a source value.
    pub fn store_message(message: impl Into<String>) -> Self {
        Self::Store {
            message: message.into(),
            source: None,
        }
    }
}

/// Persisted durable job state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting for its scheduled time and a worker lease.
    Pending,
    /// Leased by a worker and potentially executing.
    Running,
    /// A worker acknowledged successful completion.
    Succeeded,
    /// Retry policy was exhausted or the failure was permanent.
    DeadLettered,
    /// Explicitly cancelled before acknowledgement.
    Cancelled,
}

impl JobStatus {
    /// Returns the stable storage representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::DeadLettered => "dead_lettered",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a storage representation.
    pub fn from_storage(value: &str) -> DurableResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "dead_lettered" => Ok(Self::DeadLettered),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(DurableJobError::Configuration(format!(
                "unknown persisted job status `{value}`"
            ))),
        }
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A validated job ready to enqueue.
#[derive(Clone)]
pub struct NewJob {
    id: String,
    name: String,
    payload: Value,
    max_attempts: u32,
    available_at_ms: i64,
    idempotency_key: Option<String>,
    correlation_id: Option<String>,
}

impl fmt::Debug for NewJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewJob")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("payload", &"<redacted>")
            .field("max_attempts", &self.max_attempts)
            .field("available_at_ms", &self.available_at_ms)
            .field(
                "idempotency_key",
                &self.idempotency_key.as_ref().map(|_| "<redacted>"),
            )
            .field("correlation_id", &self.correlation_id)
            .finish()
    }
}

impl NewJob {
    /// Creates an immediately available job with five maximum attempts.
    pub fn new(name: impl Into<String>, payload: Value) -> DurableResult<Self> {
        let name = name.into();
        validate_identifier("job name", &name)?;
        validate_payload(&payload)?;

        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            payload,
            max_attempts: 5,
            available_at_ms: unix_timestamp_ms(),
            idempotency_key: None,
            correlation_id: None,
        })
    }

    /// Overrides the generated job identifier.
    pub fn with_id(mut self, id: impl Into<String>) -> DurableResult<Self> {
        let id = id.into();
        validate_identifier("job id", &id)?;
        self.id = id;
        Ok(self)
    }

    /// Sets the maximum number of delivery attempts, including the first.
    pub fn with_max_attempts(mut self, max_attempts: u32) -> DurableResult<Self> {
        if max_attempts == 0 {
            return Err(DurableJobError::Configuration(
                "max_attempts must be greater than zero".to_owned(),
            ));
        }
        self.max_attempts = max_attempts;
        Ok(self)
    }

    /// Schedules the job at a Unix timestamp in milliseconds.
    pub fn scheduled_at_ms(mut self, available_at_ms: i64) -> Self {
        self.available_at_ms = available_at_ms;
        self
    }

    /// Sets a backend-enforced idempotency key scoped to the job name.
    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> DurableResult<Self> {
        let key = key.into();
        validate_identifier("idempotency key", &key)?;
        self.idempotency_key = Some(key);
        Ok(self)
    }

    /// Sets a correlation identifier propagated to execution context.
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> DurableResult<Self> {
        let id = id.into();
        validate_identifier("correlation id", &id)?;
        self.correlation_id = Some(id);
        Ok(self)
    }

    /// Returns the job identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the registered handler name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the JSON payload.
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Returns the maximum attempt count.
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// Returns the scheduled Unix timestamp in milliseconds.
    pub const fn available_at_ms(&self) -> i64 {
        self.available_at_ms
    }

    /// Returns the optional idempotency key.
    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref()
    }

    /// Returns the optional correlation identifier.
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
}

/// Persisted durable job data returned by a store.
#[derive(Clone)]
pub struct DurableJobRecord {
    /// Unique job identifier.
    pub id: String,
    /// Registered handler name.
    pub name: String,
    /// User payload.
    pub payload: Value,
    /// Current durable state.
    pub status: JobStatus,
    /// Number of leases acquired so far.
    pub attempts: u32,
    /// Maximum delivery attempts.
    pub max_attempts: u32,
    /// Next eligible Unix timestamp in milliseconds.
    pub available_at_ms: i64,
    /// Current lease owner, if running.
    pub lease_owner: Option<String>,
    /// Lease expiry as a Unix timestamp in milliseconds.
    pub lease_until_ms: Option<i64>,
    /// Optional idempotency key scoped to the job name.
    pub idempotency_key: Option<String>,
    /// Optional correlation identifier.
    pub correlation_id: Option<String>,
    /// Last safe, redacted error message.
    pub last_error: Option<String>,
    /// Creation time as a Unix timestamp in milliseconds.
    pub created_at_ms: i64,
    /// Last update time as a Unix timestamp in milliseconds.
    pub updated_at_ms: i64,
}

impl fmt::Debug for DurableJobRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DurableJobRecord")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("payload", &"<redacted>")
            .field("status", &self.status)
            .field("attempts", &self.attempts)
            .field("max_attempts", &self.max_attempts)
            .field("available_at_ms", &self.available_at_ms)
            .field("lease_owner", &self.lease_owner)
            .field("lease_until_ms", &self.lease_until_ms)
            .field(
                "idempotency_key",
                &self.idempotency_key.as_ref().map(|_| "<redacted>"),
            )
            .field("correlation_id", &self.correlation_id)
            .field("last_error", &self.last_error)
            .field("created_at_ms", &self.created_at_ms)
            .field("updated_at_ms", &self.updated_at_ms)
            .finish()
    }
}

/// Result of an idempotent enqueue operation.
#[derive(Clone, Debug)]
pub enum EnqueueResult {
    /// A new job was persisted.
    Enqueued(DurableJobRecord),
    /// The existing job for the same name and idempotency key was returned.
    Duplicate(DurableJobRecord),
}

impl EnqueueResult {
    /// Returns the new or pre-existing job record.
    pub const fn record(&self) -> &DurableJobRecord {
        match self {
            Self::Enqueued(record) | Self::Duplicate(record) => record,
        }
    }

    /// Returns whether a new durable record was inserted.
    pub const fn was_enqueued(&self) -> bool {
        matches!(self, Self::Enqueued(_))
    }
}

/// Parameters for an atomic lease operation.
#[derive(Clone, Debug)]
pub struct LeaseRequest {
    worker_id: String,
    now_ms: i64,
    lease_until_ms: i64,
    limit: usize,
}

impl LeaseRequest {
    /// Creates a validated lease request.
    pub fn new(
        worker_id: impl Into<String>,
        now_ms: i64,
        lease_duration: Duration,
        limit: usize,
    ) -> DurableResult<Self> {
        let worker_id = worker_id.into();
        validate_identifier("worker id", &worker_id)?;
        if limit == 0 || limit > MAX_WORKER_CONCURRENCY {
            return Err(DurableJobError::Configuration(
                "lease limit must be between 1 and 1024".to_owned(),
            ));
        }
        let lease_ms = duration_millis_i64(lease_duration)?;
        if lease_ms == 0 {
            return Err(DurableJobError::Configuration(
                "lease duration must be greater than zero".to_owned(),
            ));
        }
        let lease_until_ms = now_ms.checked_add(lease_ms).ok_or_else(|| {
            DurableJobError::Configuration("lease timestamp overflowed".to_owned())
        })?;
        Ok(Self {
            worker_id,
            now_ms,
            lease_until_ms,
            limit,
        })
    }

    /// Returns the worker identifier.
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Returns the current Unix timestamp in milliseconds.
    pub const fn now_ms(&self) -> i64 {
        self.now_ms
    }

    /// Returns the requested lease expiry timestamp.
    pub const fn lease_until_ms(&self) -> i64 {
        self.lease_until_ms
    }

    /// Returns the maximum number of records to lease.
    pub const fn limit(&self) -> usize {
        self.limit
    }
}

/// Store action after an unsuccessful execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobDisposition {
    /// Make the job pending at the supplied Unix timestamp in milliseconds.
    RetryAt(i64),
    /// Move the job to its dead-letter state.
    DeadLetter,
}

/// Aggregate durable store counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreStats {
    /// Pending jobs.
    pub pending: u64,
    /// Running jobs.
    pub running: u64,
    /// Successfully acknowledged jobs.
    pub succeeded: u64,
    /// Dead-lettered jobs.
    pub dead_lettered: u64,
    /// Cancelled jobs.
    pub cancelled: u64,
}

impl StoreStats {
    /// Returns the total number of persisted jobs.
    pub const fn total(self) -> u64 {
        self.pending + self.running + self.succeeded + self.dead_lettered + self.cancelled
    }
}

/// Persistence contract for at-least-once durable jobs.
///
/// Implementations must acquire leases atomically across workers. Mutations of
/// running jobs must compare the job id, lease owner, and one-based attempt
/// generation. The attempt fence prevents a stale execution from mutating a
/// newer lease even when a worker ID is reused. A worker can crash after
/// performing a side effect but before acknowledgement, so handlers must be
/// idempotent; this API intentionally does not claim exactly-once delivery.
#[async_trait]
pub trait DurableJobStore: Send + Sync + 'static {
    /// Creates or upgrades backend-owned schema.
    async fn migrate(&self) -> DurableResult<()>;

    /// Persists a job or returns the existing idempotent record.
    async fn enqueue(&self, job: NewJob) -> DurableResult<EnqueueResult>;

    /// Atomically leases up to the requested number of eligible jobs.
    async fn lease(&self, request: LeaseRequest) -> DurableResult<Vec<DurableJobRecord>>;

    /// Acknowledges success only while the supplied worker still owns the lease.
    async fn acknowledge(&self, job_id: &str, worker_id: &str, attempt: u32)
    -> DurableResult<bool>;

    /// Applies a retry or dead-letter result only for the current lease owner.
    async fn fail(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
        safe_error: &str,
        disposition: JobDisposition,
    ) -> DurableResult<bool>;

    /// Extends a currently owned lease.
    async fn extend_lease(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
        lease_until_ms: i64,
    ) -> DurableResult<bool>;

    /// Makes expired running jobs eligible for another worker.
    async fn recover_expired_leases(&self, now_ms: i64) -> DurableResult<u64>;

    /// Cancels a pending or running job and clears its lease.
    async fn cancel(&self, job_id: &str) -> DurableResult<bool>;

    /// Returns aggregate state counts for health and dashboards.
    async fn stats(&self) -> DurableResult<StoreStats>;

    /// Returns dead-lettered records, newest first, up to `limit`.
    async fn dead_letters(&self, limit: usize) -> DurableResult<Vec<DurableJobRecord>>;
}

/// Safe handler failure used to choose retry or dead-letter behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobExecutionError {
    message: String,
    retryable: bool,
}

impl JobExecutionError {
    /// Creates a retryable failure.
    ///
    /// The message is persisted and must already be stripped of secrets and PII.
    pub fn retryable(message: impl Into<String>) -> Self {
        Self::new(message, true)
    }

    /// Creates a permanent failure.
    ///
    /// The message is persisted and must already be stripped of secrets and PII.
    pub fn permanent(message: impl Into<String>) -> Self {
        Self::new(message, false)
    }

    fn new(message: impl Into<String>, retryable: bool) -> Self {
        let mut message = message.into();
        if message.len() > MAX_ERROR_BYTES {
            let mut boundary = MAX_ERROR_BYTES;
            while !message.is_char_boundary(boundary) {
                boundary -= 1;
            }
            message.truncate(boundary);
        }
        Self { message, retryable }
    }

    /// Returns the safe persisted error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns whether another attempt is allowed by the handler.
    pub const fn is_retryable(&self) -> bool {
        self.retryable
    }
}

impl fmt::Display for JobExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for JobExecutionError {}

/// Context supplied to a durable job handler.
#[derive(Clone, Debug)]
pub struct JobExecutionContext {
    job: DurableJobRecord,
    cancellation: CancellationToken,
}

impl JobExecutionContext {
    fn new(job: DurableJobRecord, cancellation: CancellationToken) -> Self {
        Self { job, cancellation }
    }

    /// Returns the leased job record.
    pub const fn job(&self) -> &DurableJobRecord {
        &self.job
    }

    /// Returns the JSON payload.
    pub const fn payload(&self) -> &Value {
        &self.job.payload
    }

    /// Returns the one-based attempt number.
    pub const fn attempt(&self) -> u32 {
        self.job.attempts
    }

    /// Returns the optional correlation identifier.
    pub fn correlation_id(&self) -> Option<&str> {
        self.job.correlation_id.as_deref()
    }

    /// Returns a token cancelled during graceful worker shutdown or lease loss.
    pub const fn cancellation_token(&self) -> &CancellationToken {
        &self.cancellation
    }
}

/// Named durable job handler.
///
/// Handlers must be idempotent and cancellation-safe because leases provide
/// at-least-once, not exactly-once, execution.
#[async_trait]
pub trait DurableJobHandler: Send + Sync + 'static {
    /// Stable name persisted with each job.
    fn name(&self) -> &'static str;

    /// Executes a leased job.
    async fn execute(
        &self,
        context: JobExecutionContext,
    ) -> std::result::Result<(), JobExecutionError>;
}

/// Immutable handler registry shared by workers.
#[derive(Clone, Default)]
pub struct DurableJobRegistry {
    handlers: BTreeMap<String, Arc<dyn DurableJobHandler>>,
}

impl fmt::Debug for DurableJobRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DurableJobRegistry")
            .field("handler_names", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl DurableJobRegistry {
    /// Creates an empty registry.
    pub const fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
        }
    }

    /// Registers a handler, rejecting duplicate names.
    pub fn register<H>(&mut self, handler: H) -> DurableResult<()>
    where
        H: DurableJobHandler,
    {
        let name = handler.name();
        validate_identifier("handler name", name)?;
        if self.handlers.contains_key(name) {
            return Err(DurableJobError::Configuration(format!(
                "duplicate durable job handler `{name}`"
            )));
        }
        self.handlers.insert(name.to_owned(), Arc::new(handler));
        Ok(())
    }

    /// Returns whether a handler is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Returns the number of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Returns whether no handlers are registered.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    fn get(&self, name: &str) -> Option<Arc<dyn DurableJobHandler>> {
        self.handlers.get(name).cloned()
    }
}

/// Exponential full-jitter retry policy.
#[derive(Clone, Copy, Debug)]
pub struct JobRetryPolicy {
    base: Duration,
    maximum: Duration,
}

impl JobRetryPolicy {
    /// Creates a validated exponential full-jitter policy.
    pub fn new(base: Duration, maximum: Duration) -> DurableResult<Self> {
        if base.is_zero() {
            return Err(DurableJobError::Configuration(
                "retry base duration must be greater than zero".to_owned(),
            ));
        }
        if maximum < base {
            return Err(DurableJobError::Configuration(
                "retry maximum must not be shorter than its base".to_owned(),
            ));
        }
        Ok(Self { base, maximum })
    }

    /// Returns the full-jitter delay for a one-based attempt number.
    pub fn delay(self, attempt: u32) -> Duration {
        let cap = self.maximum_delay(attempt);
        let cap_ms = u64::try_from(cap.as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(fastrand::u64(0..=cap_ms))
    }

    /// Returns the exponential upper bound before jitter.
    pub fn maximum_delay(self, attempt: u32) -> Duration {
        let shift = attempt.saturating_sub(1).min(31);
        self.base
            .checked_mul(1_u32 << shift)
            .unwrap_or(self.maximum)
            .min(self.maximum)
    }
}

impl Default for JobRetryPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(250),
            maximum: Duration::from_secs(30),
        }
    }
}

/// Bounded worker configuration.
#[derive(Clone, Debug)]
pub struct WorkerConfig {
    worker_id: String,
    concurrency: usize,
    lease_batch_size: usize,
    lease_duration: Duration,
    heartbeat_interval: Duration,
    poll_interval: Duration,
    shutdown_grace: Duration,
    retry_policy: JobRetryPolicy,
}

impl WorkerConfig {
    /// Creates defaults with a unique worker identifier.
    pub fn new() -> Self {
        Self {
            worker_id: uuid::Uuid::new_v4().to_string(),
            concurrency: 16,
            lease_batch_size: 16,
            lease_duration: Duration::from_secs(60),
            heartbeat_interval: Duration::from_secs(20),
            poll_interval: Duration::from_millis(250),
            shutdown_grace: Duration::from_secs(30),
            retry_policy: JobRetryPolicy::default(),
        }
    }

    /// Sets a stable worker identifier used to own leases.
    pub fn with_worker_id(mut self, worker_id: impl Into<String>) -> DurableResult<Self> {
        let worker_id = worker_id.into();
        validate_identifier("worker id", &worker_id)?;
        self.worker_id = worker_id;
        Ok(self)
    }

    /// Sets the maximum number of concurrently executing jobs.
    pub fn with_concurrency(mut self, concurrency: usize) -> DurableResult<Self> {
        if concurrency == 0 || concurrency > MAX_WORKER_CONCURRENCY {
            return Err(DurableJobError::Configuration(
                "worker concurrency must be between 1 and 1024".to_owned(),
            ));
        }
        self.concurrency = concurrency;
        Ok(self)
    }

    /// Sets the maximum number of jobs leased per store call.
    pub fn with_lease_batch_size(mut self, size: usize) -> DurableResult<Self> {
        if size == 0 || size > MAX_WORKER_CONCURRENCY {
            return Err(DurableJobError::Configuration(
                "lease batch size must be between 1 and 1024".to_owned(),
            ));
        }
        self.lease_batch_size = size;
        Ok(self)
    }

    /// Sets lease and heartbeat durations.
    pub fn with_lease_timing(
        mut self,
        lease_duration: Duration,
        heartbeat_interval: Duration,
    ) -> DurableResult<Self> {
        if lease_duration.is_zero() || heartbeat_interval.is_zero() {
            return Err(DurableJobError::Configuration(
                "lease and heartbeat durations must be greater than zero".to_owned(),
            ));
        }
        if heartbeat_interval >= lease_duration {
            return Err(DurableJobError::Configuration(
                "heartbeat interval must be shorter than lease duration".to_owned(),
            ));
        }
        self.lease_duration = lease_duration;
        self.heartbeat_interval = heartbeat_interval;
        Ok(self)
    }

    /// Sets the idle poll interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> DurableResult<Self> {
        if interval.is_zero() {
            return Err(DurableJobError::Configuration(
                "poll interval must be greater than zero".to_owned(),
            ));
        }
        self.poll_interval = interval;
        Ok(self)
    }

    /// Sets the maximum graceful drain duration.
    pub fn with_shutdown_grace(mut self, grace: Duration) -> Self {
        self.shutdown_grace = grace;
        self
    }

    /// Sets the handler retry policy.
    pub const fn with_retry_policy(mut self, policy: JobRetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Returns the lease owner identifier.
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Returns the execution concurrency bound.
    pub const fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Returns the per-call lease batch bound.
    pub const fn lease_batch_size(&self) -> usize {
        self.lease_batch_size
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregate results from a worker run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerReport {
    /// Jobs leased by this worker.
    pub leased: u64,
    /// Jobs successfully acknowledged.
    pub succeeded: u64,
    /// Jobs scheduled for another attempt.
    pub retried: u64,
    /// Jobs moved to dead-letter state.
    pub dead_lettered: u64,
    /// Jobs whose lease was lost before a terminal write.
    pub lease_lost: u64,
    /// Store operations that failed after startup.
    pub store_errors: u64,
    /// Tasks abandoned after the shutdown grace period.
    pub abandoned: u64,
}

/// Bounded, multi-worker-safe durable job executor.
pub struct DurableJobWorker {
    store: Arc<dyn DurableJobStore>,
    registry: DurableJobRegistry,
    config: WorkerConfig,
}

impl fmt::Debug for DurableJobWorker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DurableJobWorker")
            .field("registry", &self.registry)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl DurableJobWorker {
    /// Creates a worker over a first-party or custom durable store.
    pub fn new<S>(store: Arc<S>, registry: DurableJobRegistry, config: WorkerConfig) -> Self
    where
        S: DurableJobStore,
    {
        Self {
            store,
            registry,
            config,
        }
    }

    /// Creates a worker from a dynamically dispatched store.
    pub fn from_dyn(
        store: Arc<dyn DurableJobStore>,
        registry: DurableJobRegistry,
        config: WorkerConfig,
    ) -> Self {
        Self {
            store,
            registry,
            config,
        }
    }

    /// Migrates the store, recovers expired leases, and runs until cancelled.
    ///
    /// Cancellation stops new leasing immediately and is propagated to handler
    /// contexts. Active tasks may drain until the configured grace period.
    pub async fn run(self, shutdown: CancellationToken) -> DurableResult<WorkerReport> {
        self.store.migrate().await?;
        self.store
            .recover_expired_leases(unix_timestamp_ms())
            .await?;

        let mut tasks = JoinSet::new();
        let mut report = WorkerReport::default();
        let mut consecutive_store_errors = 0_u32;
        let mut next_recovery = Instant::now() + recovery_interval(self.config.lease_duration);

        while !shutdown.is_cancelled() {
            while let Some(result) = tasks.try_join_next() {
                apply_task_result(result, &mut report)?;
            }

            if Instant::now() >= next_recovery {
                match self.store.recover_expired_leases(unix_timestamp_ms()).await {
                    Ok(_) => consecutive_store_errors = 0,
                    Err(error) => {
                        report.store_errors += 1;
                        consecutive_store_errors = consecutive_store_errors.saturating_add(1);
                        tracing::warn!(error = %error, "failed to recover expired durable job leases");
                    }
                }
                next_recovery = Instant::now() + recovery_interval(self.config.lease_duration);
            }

            let capacity = self.config.concurrency.saturating_sub(tasks.len());
            if capacity == 0 {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    result = tasks.join_next() => {
                        if let Some(result) = result {
                            apply_task_result(result, &mut report)?;
                        }
                    }
                }
                continue;
            }

            let request = LeaseRequest::new(
                self.config.worker_id.clone(),
                unix_timestamp_ms(),
                self.config.lease_duration,
                capacity.min(self.config.lease_batch_size),
            )?;
            let leased = match self.store.lease(request).await {
                Ok(leased) => {
                    consecutive_store_errors = 0;
                    leased
                }
                Err(error) => {
                    report.store_errors += 1;
                    consecutive_store_errors = consecutive_store_errors.saturating_add(1);
                    tracing::warn!(error = %error, "failed to lease durable jobs");
                    let delay = self.config.retry_policy.delay(consecutive_store_errors);
                    wait_or_cancel(delay, &shutdown).await;
                    continue;
                }
            };

            if leased.is_empty() {
                wait_or_cancel(self.config.poll_interval, &shutdown).await;
                continue;
            }

            report.leased += u64::try_from(leased.len()).unwrap_or(u64::MAX);
            for job in leased {
                let task = ExecutionTask {
                    store: Arc::clone(&self.store),
                    handler: self.registry.get(&job.name),
                    job,
                    worker_id: self.config.worker_id.clone(),
                    lease_duration: self.config.lease_duration,
                    heartbeat_interval: self.config.heartbeat_interval,
                    retry_policy: self.config.retry_policy,
                    cancellation: shutdown.child_token(),
                };
                tasks.spawn(task.run());
            }
        }

        drain_tasks(&mut tasks, &mut report, self.config.shutdown_grace).await?;
        Ok(report)
    }
}

struct ExecutionTask {
    store: Arc<dyn DurableJobStore>,
    handler: Option<Arc<dyn DurableJobHandler>>,
    job: DurableJobRecord,
    worker_id: String,
    lease_duration: Duration,
    heartbeat_interval: Duration,
    retry_policy: JobRetryPolicy,
    cancellation: CancellationToken,
}

impl ExecutionTask {
    async fn run(self) -> TaskOutcome {
        let Some(handler) = self.handler.clone() else {
            return self
                .finish_failure(JobExecutionError::permanent(
                    "no handler registered for durable job",
                ))
                .await;
        };

        let handler_cancellation = self.cancellation.child_token();
        let context = JobExecutionContext::new(self.job.clone(), handler_cancellation.clone());
        let handler_future = AssertUnwindSafe(handler.execute(context)).catch_unwind();
        tokio::pin!(handler_future);
        let mut heartbeat = tokio::time::interval(self.heartbeat_interval);
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        heartbeat.tick().await;

        let result = loop {
            tokio::select! {
                result = &mut handler_future => break result,
                _ = heartbeat.tick() => {
                    let Some(lease_until_ms) = unix_timestamp_ms().checked_add(
                        duration_millis_i64(self.lease_duration).unwrap_or(i64::MAX),
                    ) else {
                        handler_cancellation.cancel();
                        return TaskOutcome::StoreError;
                    };
                    match self.store.extend_lease(
                        &self.job.id,
                        &self.worker_id,
                        self.job.attempts,
                        lease_until_ms,
                    ).await {
                        Ok(true) => {}
                        Ok(false) => {
                            handler_cancellation.cancel();
                            return TaskOutcome::LeaseLost;
                        }
                        Err(error) => {
                            tracing::warn!(
                                job_id = %self.job.id,
                                error = %error,
                                "failed to extend durable job lease"
                            );
                            handler_cancellation.cancel();
                            return TaskOutcome::StoreError;
                        }
                    }
                }
            }
        };

        match result {
            Ok(Ok(())) => match self
                .store
                .acknowledge(&self.job.id, &self.worker_id, self.job.attempts)
                .await
            {
                Ok(true) => TaskOutcome::Succeeded,
                Ok(false) => TaskOutcome::LeaseLost,
                Err(error) => {
                    tracing::warn!(
                        job_id = %self.job.id,
                        error = %error,
                        "failed to acknowledge durable job"
                    );
                    TaskOutcome::StoreError
                }
            },
            Ok(Err(error)) => self.finish_failure(error).await,
            Err(_) => {
                self.finish_failure(JobExecutionError::retryable("durable job handler panicked"))
                    .await
            }
        }
    }

    async fn finish_failure(self, error: JobExecutionError) -> TaskOutcome {
        let retry = error.is_retryable() && self.job.attempts < self.job.max_attempts;
        let disposition = if retry {
            let delay = self.retry_policy.delay(self.job.attempts);
            let retry_at = unix_timestamp_ms()
                .saturating_add(i64::try_from(delay.as_millis()).unwrap_or(i64::MAX));
            JobDisposition::RetryAt(retry_at)
        } else {
            JobDisposition::DeadLetter
        };

        match self
            .store
            .fail(
                &self.job.id,
                &self.worker_id,
                self.job.attempts,
                error.message(),
                disposition,
            )
            .await
        {
            Ok(true) if retry => TaskOutcome::Retried,
            Ok(true) => TaskOutcome::DeadLettered,
            Ok(false) => TaskOutcome::LeaseLost,
            Err(store_error) => {
                tracing::warn!(
                    job_id = %self.job.id,
                    error = %store_error,
                    "failed to persist durable job failure"
                );
                TaskOutcome::StoreError
            }
        }
    }
}

enum TaskOutcome {
    Succeeded,
    Retried,
    DeadLettered,
    LeaseLost,
    StoreError,
}

fn apply_task_result(
    result: std::result::Result<TaskOutcome, tokio::task::JoinError>,
    report: &mut WorkerReport,
) -> DurableResult<()> {
    match result {
        Ok(TaskOutcome::Succeeded) => report.succeeded += 1,
        Ok(TaskOutcome::Retried) => report.retried += 1,
        Ok(TaskOutcome::DeadLettered) => report.dead_lettered += 1,
        Ok(TaskOutcome::LeaseLost) => report.lease_lost += 1,
        Ok(TaskOutcome::StoreError) => report.store_errors += 1,
        Err(error) if error.is_cancelled() => report.abandoned += 1,
        Err(error) => return Err(DurableJobError::TaskJoin(error.to_string())),
    }
    Ok(())
}

async fn drain_tasks(
    tasks: &mut JoinSet<TaskOutcome>,
    report: &mut WorkerReport,
    grace: Duration,
) -> DurableResult<()> {
    let deadline = Instant::now() + grace;
    while !tasks.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, tasks.join_next()).await {
            Ok(Some(result)) => apply_task_result(result, report)?,
            Ok(None) => break,
            Err(_) => break,
        }
    }

    if !tasks.is_empty() {
        report.abandoned += u64::try_from(tasks.len()).unwrap_or(u64::MAX);
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
    Ok(())
}

async fn wait_or_cancel(duration: Duration, cancellation: &CancellationToken) {
    tokio::select! {
        () = tokio::time::sleep(duration) => {}
        () = cancellation.cancelled() => {}
    }
}

fn recovery_interval(lease_duration: Duration) -> Duration {
    lease_duration
        .checked_div(2)
        .unwrap_or(Duration::from_secs(1))
        .max(Duration::from_secs(1))
}

fn validate_identifier(field: &str, value: &str) -> DurableResult<()> {
    if value.is_empty() {
        return Err(DurableJobError::Configuration(format!(
            "{field} must not be empty"
        )));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(DurableJobError::Configuration(format!(
            "{field} exceeds {MAX_IDENTIFIER_BYTES} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(DurableJobError::Configuration(format!(
            "{field} contains control characters"
        )));
    }
    Ok(())
}

fn validate_payload(payload: &Value) -> DurableResult<()> {
    let mut writer = PayloadSizeWriter::default();
    if let Err(error) = serde_json::to_writer(&mut writer, payload) {
        if writer.exceeded {
            return Err(DurableJobError::Configuration(format!(
                "job payload exceeds {MAX_JOB_PAYLOAD_BYTES} bytes"
            )));
        }
        return Err(error.into());
    }
    Ok(())
}

#[derive(Default)]
struct PayloadSizeWriter {
    written: usize,
    exceeded: bool,
}

impl io::Write for PayloadSizeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(total) = self.written.checked_add(bytes.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("durable job payload size overflowed"));
        };
        if total > MAX_JOB_PAYLOAD_BYTES {
            self.exceeded = true;
            return Err(io::Error::other("durable job payload exceeds its limit"));
        }
        self.written = total;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn duration_millis_i64(duration: Duration) -> DurableResult<i64> {
    i64::try_from(duration.as_millis()).map_err(|_| {
        DurableJobError::Configuration("duration does not fit in milliseconds".to_owned())
    })
}

fn unix_timestamp_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

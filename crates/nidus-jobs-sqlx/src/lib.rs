#![deny(missing_docs)]

//! First-party SQLx persistence for [`nidus_jobs`] durable workers.
//!
//! The store exposes its native [`sqlx::AnyPool`] and keeps execution semantics
//! at least once. Atomic conditional updates make lease ownership safe across
//! workers, but handlers remain responsible for idempotent side effects.

use std::{fmt, time::Instant};

use async_trait::async_trait;
use nidus_core::{Container, LifecycleHook, NidusError};
use nidus_integrations::{IntegrationEvent, IntegrationStatus, IntegrationTelemetry};
use nidus_jobs::{
    DurableJobError, DurableJobRecord, DurableJobStore, DurableResult, EnqueueResult,
    JobDisposition, JobStatus, LeaseRequest, NewJob, StoreStats,
};
use sqlx::{AnyPool, Row, any::AnyPoolOptions};

const TABLE: &str = "nidus_jobs";
const MAX_QUERY_LIMIT: usize = 1_024;

/// SQL dialect used by the durable job schema and parameter renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqlDialect {
    /// SQLite 3.
    Sqlite,
    /// PostgreSQL.
    Postgres,
    /// CockroachDB through its PostgreSQL wire protocol.
    Cockroach,
    /// MySQL.
    MySql,
}

impl SqlDialect {
    fn placeholder(self, index: usize) -> String {
        match self {
            Self::Postgres | Self::Cockroach => format!("${index}"),
            Self::Sqlite | Self::MySql => "?".to_owned(),
        }
    }
}

/// Redaction-safe SQLx durable store configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct SqlxJobStoreConfig {
    database_url: String,
    dialect: SqlDialect,
    max_connections: u32,
    allow_insecure_local: bool,
}

impl SqlxJobStoreConfig {
    /// Creates explicit configuration for a SQL dialect.
    pub fn new(database_url: impl Into<String>, dialect: SqlDialect) -> Self {
        Self {
            database_url: database_url.into(),
            dialect,
            max_connections: 10,
            allow_insecure_local: false,
        }
    }

    /// Creates SQLite configuration.
    pub fn sqlite(database_url: impl Into<String>) -> Self {
        Self::new(database_url, SqlDialect::Sqlite)
    }

    /// Creates PostgreSQL configuration.
    pub fn postgres(database_url: impl Into<String>) -> Self {
        Self::new(database_url, SqlDialect::Postgres)
    }

    /// Creates CockroachDB configuration requiring `sslmode=verify-full`.
    pub fn cockroach(database_url: impl Into<String>) -> Self {
        Self::new(database_url, SqlDialect::Cockroach)
    }

    /// Creates MySQL configuration.
    pub fn mysql(database_url: impl Into<String>) -> Self {
        Self::new(database_url, SqlDialect::MySql)
    }

    /// Sets the connection pool bound.
    pub fn with_max_connections(mut self, max_connections: u32) -> DurableResult<Self> {
        if max_connections == 0 {
            return Err(DurableJobError::Configuration(
                "SQLx job pool max_connections must be greater than zero".to_owned(),
            ));
        }
        self.max_connections = max_connections;
        Ok(self)
    }

    /// Allows a plaintext CockroachDB URL only when it targets the loopback host.
    ///
    /// This escape hatch is intended for hermetic local tests, never production.
    pub fn allow_insecure_local_cockroach(mut self) -> DurableResult<Self> {
        if self.dialect != SqlDialect::Cockroach
            || !has_dialect_scheme(self.dialect, &self.database_url)
            || !is_loopback_url(&self.database_url)
        {
            return Err(DurableJobError::Configuration(
                "insecure CockroachDB is restricted to explicit loopback URLs".to_owned(),
            ));
        }
        self.allow_insecure_local = true;
        Ok(self)
    }

    /// Allows a plaintext PostgreSQL URL only when it targets the loopback host.
    pub fn allow_insecure_local_postgres(mut self) -> DurableResult<Self> {
        if self.dialect != SqlDialect::Postgres
            || !has_dialect_scheme(self.dialect, &self.database_url)
            || !is_loopback_url(&self.database_url)
        {
            return Err(DurableJobError::Configuration(
                "insecure PostgreSQL is restricted to explicit loopback URLs".to_owned(),
            ));
        }
        self.allow_insecure_local = true;
        Ok(self)
    }

    /// Allows a plaintext MySQL URL only when it targets the loopback host.
    pub fn allow_insecure_local_mysql(mut self) -> DurableResult<Self> {
        if self.dialect != SqlDialect::MySql
            || !has_dialect_scheme(self.dialect, &self.database_url)
            || !is_loopback_url(&self.database_url)
        {
            return Err(DurableJobError::Configuration(
                "insecure MySQL is restricted to explicit loopback URLs".to_owned(),
            ));
        }
        self.allow_insecure_local = true;
        Ok(self)
    }

    /// Returns the selected SQL dialect.
    pub const fn dialect(&self) -> SqlDialect {
        self.dialect
    }

    /// Returns the maximum pool connection count.
    pub const fn max_connections(&self) -> u32 {
        self.max_connections
    }

    fn validate(&self) -> DurableResult<()> {
        if self.database_url.trim().is_empty() {
            return Err(DurableJobError::Configuration(
                "SQLx job database URL must not be empty".to_owned(),
            ));
        }
        if !has_dialect_scheme(self.dialect, &self.database_url) {
            return Err(DurableJobError::Configuration(
                "SQLx job database URL scheme does not match the selected dialect".to_owned(),
            ));
        }
        if matches!(self.dialect, SqlDialect::Postgres | SqlDialect::Cockroach)
            && !has_verify_full(&self.database_url)
            && !self.allow_insecure_local
        {
            return Err(DurableJobError::Configuration(
                "PostgreSQL and CockroachDB require TLS hostname verification with sslmode=verify-full"
                    .to_owned(),
            ));
        }
        if self.dialect == SqlDialect::MySql
            && !has_mysql_verify_identity(&self.database_url)
            && !self.allow_insecure_local
        {
            return Err(DurableJobError::Configuration(
                "MySQL requires ssl-mode=VERIFY_IDENTITY except for explicit loopback development"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for SqlxJobStoreConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqlxJobStoreConfig")
            .field("database_url", &"<redacted>")
            .field("dialect", &self.dialect)
            .field("max_connections", &self.max_connections)
            .field("allow_insecure_local", &self.allow_insecure_local)
            .finish()
    }
}

/// SQLx-backed durable job store with native pool access.
#[derive(Clone)]
pub struct SqlxJobStore {
    pool: AnyPool,
    dialect: SqlDialect,
    telemetry: IntegrationTelemetry,
}

impl SqlxJobStore {
    /// Connects a bounded native SQLx pool after validating secure defaults.
    pub async fn connect(config: SqlxJobStoreConfig) -> DurableResult<Self> {
        config.validate()?;
        sqlx::any::install_default_drivers();
        let started = Instant::now();
        let result = AnyPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.database_url)
            .await;
        let success = result.is_ok();
        let store = Self {
            pool: result.map_err(|error| DurableJobError::store("connect", error))?,
            dialect: config.dialect,
            telemetry: IntegrationTelemetry::new(),
        };
        store.record("connect", success, started).await;
        Ok(store)
    }

    /// Wraps an application-owned native SQLx pool.
    ///
    /// Callers selecting [`SqlDialect::Cockroach`] are responsible for building
    /// the pool with certificate and hostname verification enabled.
    pub fn from_pool(pool: AnyPool, dialect: SqlDialect) -> Self {
        Self {
            pool,
            dialect,
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Adds shared tracing, metrics, or dashboard observers.
    pub fn with_telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Returns the native SQLx pool for backend-specific operations.
    pub const fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Returns the selected SQL dialect.
    pub const fn dialect(&self) -> SqlDialect {
        self.dialect
    }

    /// Registers this store as a typed singleton dependency.
    pub fn register(&self, container: &mut Container) -> nidus_core::Result<()> {
        container.register_singleton(self.clone())
    }

    /// Runs a readiness query and returns dashboard-friendly state counts.
    pub async fn health(&self) -> DurableResult<StoreStats> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("health query", error))?;
        self.stats().await
    }

    /// Executes the native readiness probe and returns a redaction-safe status.
    #[cfg(feature = "health")]
    pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
        if self.health().await.is_ok() {
            nidus_http::health::HealthStatus::up()
        } else {
            nidus_http::health::HealthStatus::down("durable job store readiness check failed")
        }
    }

    /// Adds this durable store as a readiness check.
    #[cfg(feature = "health")]
    pub fn register_ready_check(
        self: std::sync::Arc<Self>,
        registry: nidus_http::health::HealthRegistry,
        name: impl Into<String>,
    ) -> nidus_http::health::HealthRegistry {
        registry.ready_check(name, move || {
            let store = std::sync::Arc::clone(&self);
            async move { store.health_status().await }
        })
    }

    /// Gracefully closes the native pool.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn record(&self, operation: &'static str, success: bool, started: Instant) {
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-jobs-sqlx",
                operation,
                if success {
                    IntegrationStatus::Success
                } else {
                    IntegrationStatus::Failure
                },
                started.elapsed(),
            ))
            .await;
    }

    async fn fetch_by_id(&self, id: &str) -> DurableResult<Option<DurableJobRecord>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM {TABLE} WHERE id = {}",
            self.dialect.placeholder(1)
        );
        sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("fetch job by id", error))?
            .map(record_from_row)
            .transpose()
    }

    async fn fetch_by_idempotency_key(
        &self,
        name: &str,
        key: &str,
    ) -> DurableResult<Option<DurableJobRecord>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM {TABLE} WHERE name = {} AND idempotency_key = {}",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2)
        );
        sqlx::query(&sql)
            .bind(name)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("fetch idempotent job", error))?
            .map(record_from_row)
            .transpose()
    }

    async fn enqueue_inner(&self, job: NewJob) -> DurableResult<EnqueueResult> {
        if let Some(key) = job.idempotency_key()
            && let Some(existing) = self.fetch_by_idempotency_key(job.name(), key).await?
        {
            return Ok(EnqueueResult::Duplicate(existing));
        }

        let payload_json = serde_json::to_string(job.payload())?;
        let now_ms = unix_timestamp_ms();
        let values = (1..=9)
            .map(|index| self.dialect.placeholder(index))
            .collect::<Vec<_>>();
        let sql = format!(
            "INSERT INTO {TABLE} (id, name, payload_json, status, attempts, max_attempts, \
             available_at_ms, lease_owner, lease_until_ms, idempotency_key, correlation_id, \
             last_error, created_at_ms, updated_at_ms) VALUES \
             ({}, {}, {}, 'pending', 0, {}, {}, NULL, NULL, {}, {}, NULL, {}, {})",
            values[0],
            values[1],
            values[2],
            values[3],
            values[4],
            values[5],
            values[6],
            values[7],
            values[8]
        );
        let insert = sqlx::query(&sql)
            .bind(job.id())
            .bind(job.name())
            .bind(payload_json)
            .bind(i64::from(job.max_attempts()))
            .bind(job.available_at_ms())
            .bind(job.idempotency_key())
            .bind(job.correlation_id())
            .bind(now_ms)
            .bind(now_ms)
            .execute(&self.pool)
            .await;

        if let Err(error) = insert {
            if let Some(key) = job.idempotency_key()
                && let Some(existing) = self.fetch_by_idempotency_key(job.name(), key).await?
            {
                return Ok(EnqueueResult::Duplicate(existing));
            }
            return Err(DurableJobError::store("insert durable job", error));
        }

        self.fetch_by_id(job.id())
            .await?
            .map(EnqueueResult::Enqueued)
            .ok_or_else(|| DurableJobError::store_message("inserted durable job was not found"))
    }

    async fn lease_inner(&self, request: LeaseRequest) -> DurableResult<Vec<DurableJobRecord>> {
        let limit = request.limit().min(MAX_QUERY_LIMIT);
        let candidates_sql = format!(
            "SELECT id FROM {TABLE} WHERE status = 'pending' AND available_at_ms <= {} \
             ORDER BY available_at_ms ASC, created_at_ms ASC LIMIT {limit}",
            self.dialect.placeholder(1)
        );
        let candidates = sqlx::query(&candidates_sql)
            .bind(request.now_ms())
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("select lease candidates", error))?;

        let update_sql = format!(
            "UPDATE {TABLE} SET status = 'running', lease_owner = {}, lease_until_ms = {}, \
             attempts = attempts + 1, updated_at_ms = {} WHERE id = {} AND status = 'pending' \
             AND available_at_ms <= {}",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2),
            self.dialect.placeholder(3),
            self.dialect.placeholder(4),
            self.dialect.placeholder(5)
        );
        let mut leased = Vec::with_capacity(candidates.len());
        for row in candidates {
            let id: String = row
                .try_get("id")
                .map_err(|error| DurableJobError::store("decode lease candidate", error))?;
            let updated = sqlx::query(&update_sql)
                .bind(request.worker_id())
                .bind(request.lease_until_ms())
                .bind(request.now_ms())
                .bind(&id)
                .bind(request.now_ms())
                .execute(&self.pool)
                .await
                .map_err(|error| DurableJobError::store("claim job lease", error))?;
            if updated.rows_affected() == 1
                && let Some(record) = self.fetch_by_id(&id).await?
            {
                leased.push(record);
            }
        }
        Ok(leased)
    }

    async fn acknowledge_inner(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
    ) -> DurableResult<bool> {
        let sql = format!(
            "UPDATE {TABLE} SET status = 'succeeded', lease_owner = NULL, lease_until_ms = NULL, \
             updated_at_ms = {} WHERE id = {} AND status = 'running' AND lease_owner = {} \
             AND attempts = {}",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2),
            self.dialect.placeholder(3),
            self.dialect.placeholder(4)
        );
        let result = sqlx::query(&sql)
            .bind(unix_timestamp_ms())
            .bind(job_id)
            .bind(worker_id)
            .bind(i64::from(attempt))
            .execute(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("acknowledge job", error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn fail_inner(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
        safe_error: &str,
        disposition: JobDisposition,
    ) -> DurableResult<bool> {
        let (status, available_at_ms) = match disposition {
            JobDisposition::RetryAt(timestamp) => (JobStatus::Pending, timestamp),
            JobDisposition::DeadLetter => (JobStatus::DeadLettered, unix_timestamp_ms()),
        };
        let sql = format!(
            "UPDATE {TABLE} SET status = {}, available_at_ms = {}, last_error = {}, \
             lease_owner = NULL, lease_until_ms = NULL, updated_at_ms = {} WHERE id = {} \
             AND status = 'running' AND lease_owner = {} AND attempts = {}",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2),
            self.dialect.placeholder(3),
            self.dialect.placeholder(4),
            self.dialect.placeholder(5),
            self.dialect.placeholder(6),
            self.dialect.placeholder(7)
        );
        let result = sqlx::query(&sql)
            .bind(status.as_str())
            .bind(available_at_ms)
            .bind(safe_error)
            .bind(unix_timestamp_ms())
            .bind(job_id)
            .bind(worker_id)
            .bind(i64::from(attempt))
            .execute(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("persist job failure", error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn extend_lease_inner(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
        lease_until_ms: i64,
    ) -> DurableResult<bool> {
        let sql = format!(
            "UPDATE {TABLE} SET lease_until_ms = {}, updated_at_ms = {} WHERE id = {} \
             AND status = 'running' AND lease_owner = {} AND attempts = {}",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2),
            self.dialect.placeholder(3),
            self.dialect.placeholder(4),
            self.dialect.placeholder(5)
        );
        let result = sqlx::query(&sql)
            .bind(lease_until_ms)
            .bind(unix_timestamp_ms())
            .bind(job_id)
            .bind(worker_id)
            .bind(i64::from(attempt))
            .execute(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("extend job lease", error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn recover_inner(&self, now_ms: i64) -> DurableResult<u64> {
        let sql = format!(
            "UPDATE {TABLE} SET status = 'pending', available_at_ms = {}, lease_owner = NULL, \
             lease_until_ms = NULL, updated_at_ms = {} WHERE status = 'running' \
             AND lease_until_ms < {}",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2),
            self.dialect.placeholder(3)
        );
        sqlx::query(&sql)
            .bind(now_ms)
            .bind(now_ms)
            .bind(now_ms)
            .execute(&self.pool)
            .await
            .map(|result| result.rows_affected())
            .map_err(|error| DurableJobError::store("recover expired leases", error))
    }

    async fn cancel_inner(&self, job_id: &str) -> DurableResult<bool> {
        let sql = format!(
            "UPDATE {TABLE} SET status = 'cancelled', lease_owner = NULL, lease_until_ms = NULL, \
             updated_at_ms = {} WHERE id = {} AND status IN ('pending', 'running')",
            self.dialect.placeholder(1),
            self.dialect.placeholder(2)
        );
        let result = sqlx::query(&sql)
            .bind(unix_timestamp_ms())
            .bind(job_id)
            .execute(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("cancel job", error))?;
        Ok(result.rows_affected() == 1)
    }

    async fn stats_inner(&self) -> DurableResult<StoreStats> {
        let rows = sqlx::query(&format!(
            "SELECT status, COUNT(*) AS job_count FROM {TABLE} GROUP BY status"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|error| DurableJobError::store("query job statistics", error))?;
        let mut stats = StoreStats::default();
        for row in rows {
            let status: String = row
                .try_get("status")
                .map_err(|error| DurableJobError::store("decode job status", error))?;
            let count: i64 = row
                .try_get("job_count")
                .map_err(|error| DurableJobError::store("decode job count", error))?;
            let count = u64::try_from(count)
                .map_err(|_| DurableJobError::store_message("durable job count was negative"))?;
            match JobStatus::from_storage(&status)? {
                JobStatus::Pending => stats.pending = count,
                JobStatus::Running => stats.running = count,
                JobStatus::Succeeded => stats.succeeded = count,
                JobStatus::DeadLettered => stats.dead_lettered = count,
                JobStatus::Cancelled => stats.cancelled = count,
            }
        }
        Ok(stats)
    }

    async fn dead_letters_inner(&self, limit: usize) -> DurableResult<Vec<DurableJobRecord>> {
        let limit = limit.min(MAX_QUERY_LIMIT);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT {COLUMNS} FROM {TABLE} WHERE status = 'dead_lettered' \
             ORDER BY updated_at_ms DESC LIMIT {limit}"
        );
        sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| DurableJobError::store("query dead letters", error))?
            .into_iter()
            .map(record_from_row)
            .collect()
    }
}

impl fmt::Debug for SqlxJobStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqlxJobStore")
            .field("dialect", &self.dialect)
            .field("pool", &"AnyPool")
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

#[async_trait]
impl DurableJobStore for SqlxJobStore {
    async fn migrate(&self) -> DurableResult<()> {
        let started = Instant::now();
        let result = async {
            let schema = if self.dialect == SqlDialect::MySql {
                MYSQL_SCHEMA
            } else {
                SCHEMA
            };
            sqlx::query(schema)
                .execute(&self.pool)
                .await
                .map_err(|error| DurableJobError::store("migrate durable jobs", error))?;
            if self.dialect != SqlDialect::MySql {
                for index in INDEXES {
                    sqlx::query(index)
                        .execute(&self.pool)
                        .await
                        .map_err(|error| {
                            DurableJobError::store("migrate durable job index", error)
                        })?;
                }
            }
            Ok(())
        }
        .await;
        self.record("migrate", result.is_ok(), started).await;
        result
    }

    async fn enqueue(&self, job: NewJob) -> DurableResult<EnqueueResult> {
        let started = Instant::now();
        let result = self.enqueue_inner(job).await;
        self.record("enqueue", result.is_ok(), started).await;
        result
    }

    async fn lease(&self, request: LeaseRequest) -> DurableResult<Vec<DurableJobRecord>> {
        let started = Instant::now();
        let result = self.lease_inner(request).await;
        self.record("lease", result.is_ok(), started).await;
        result
    }

    async fn acknowledge(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
    ) -> DurableResult<bool> {
        let started = Instant::now();
        let result = self.acknowledge_inner(job_id, worker_id, attempt).await;
        self.record("acknowledge", result.is_ok(), started).await;
        result
    }

    async fn fail(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
        safe_error: &str,
        disposition: JobDisposition,
    ) -> DurableResult<bool> {
        let started = Instant::now();
        let result = self
            .fail_inner(job_id, worker_id, attempt, safe_error, disposition)
            .await;
        self.record("fail", result.is_ok(), started).await;
        result
    }

    async fn extend_lease(
        &self,
        job_id: &str,
        worker_id: &str,
        attempt: u32,
        lease_until_ms: i64,
    ) -> DurableResult<bool> {
        let started = Instant::now();
        let result = self
            .extend_lease_inner(job_id, worker_id, attempt, lease_until_ms)
            .await;
        self.record("extend_lease", result.is_ok(), started).await;
        result
    }

    async fn recover_expired_leases(&self, now_ms: i64) -> DurableResult<u64> {
        let started = Instant::now();
        let result = self.recover_inner(now_ms).await;
        self.record("recover_expired_leases", result.is_ok(), started)
            .await;
        result
    }

    async fn cancel(&self, job_id: &str) -> DurableResult<bool> {
        let started = Instant::now();
        let result = self.cancel_inner(job_id).await;
        self.record("cancel", result.is_ok(), started).await;
        result
    }

    async fn stats(&self) -> DurableResult<StoreStats> {
        let started = Instant::now();
        let result = self.stats_inner().await;
        self.record("stats", result.is_ok(), started).await;
        result
    }

    async fn dead_letters(&self, limit: usize) -> DurableResult<Vec<DurableJobRecord>> {
        let started = Instant::now();
        let result = self.dead_letters_inner(limit).await;
        self.record("dead_letters", result.is_ok(), started).await;
        result
    }
}

#[async_trait]
impl LifecycleHook for SqlxJobStore {
    async fn on_startup(&self) -> nidus_core::Result<()> {
        self.migrate()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "SQLx durable job migration failed".to_owned(),
            })
    }

    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.close().await;
        Ok(())
    }
}

const COLUMNS: &str = "id, name, payload_json, status, attempts, max_attempts, available_at_ms, \
    lease_owner, lease_until_ms, idempotency_key, correlation_id, last_error, created_at_ms, \
    updated_at_ms";

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS nidus_jobs (\
    id VARCHAR(255) PRIMARY KEY,\
    name VARCHAR(255) NOT NULL,\
    payload_json TEXT NOT NULL,\
    status VARCHAR(32) NOT NULL,\
    attempts BIGINT NOT NULL,\
    max_attempts BIGINT NOT NULL,\
    available_at_ms BIGINT NOT NULL,\
    lease_owner VARCHAR(255) NULL,\
    lease_until_ms BIGINT NULL,\
    idempotency_key VARCHAR(255) NULL,\
    correlation_id VARCHAR(255) NULL,\
    last_error TEXT NULL,\
    created_at_ms BIGINT NOT NULL,\
    updated_at_ms BIGINT NOT NULL,\
    UNIQUE (name, idempotency_key)\
)";

const MYSQL_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS nidus_jobs (\
    id VARCHAR(255) PRIMARY KEY,\
    name VARCHAR(255) NOT NULL,\
    payload_json TEXT NOT NULL,\
    status VARCHAR(32) NOT NULL,\
    attempts BIGINT NOT NULL,\
    max_attempts BIGINT NOT NULL,\
    available_at_ms BIGINT NOT NULL,\
    lease_owner VARCHAR(255) NULL,\
    lease_until_ms BIGINT NULL,\
    idempotency_key VARCHAR(255) NULL,\
    correlation_id VARCHAR(255) NULL,\
    last_error TEXT NULL,\
    created_at_ms BIGINT NOT NULL,\
    updated_at_ms BIGINT NOT NULL,\
    UNIQUE (name, idempotency_key),\
    INDEX nidus_jobs_ready_idx (status, available_at_ms, created_at_ms),\
    INDEX nidus_jobs_recovery_idx (status, lease_until_ms),\
    INDEX nidus_jobs_dead_letter_idx (status, updated_at_ms)\
)";

const INDEXES: [&str; 3] = [
    "CREATE INDEX IF NOT EXISTS nidus_jobs_ready_idx \
     ON nidus_jobs (status, available_at_ms, created_at_ms)",
    "CREATE INDEX IF NOT EXISTS nidus_jobs_recovery_idx \
     ON nidus_jobs (status, lease_until_ms)",
    "CREATE INDEX IF NOT EXISTS nidus_jobs_dead_letter_idx \
     ON nidus_jobs (status, updated_at_ms)",
];

fn record_from_row(row: sqlx::any::AnyRow) -> DurableResult<DurableJobRecord> {
    let payload_json = decode_text(&row, "payload_json")?;
    let attempts: i64 = decode(&row, "attempts")?;
    let max_attempts: i64 = decode(&row, "max_attempts")?;
    Ok(DurableJobRecord {
        id: decode(&row, "id")?,
        name: decode(&row, "name")?,
        payload: serde_json::from_str(&payload_json)?,
        status: JobStatus::from_storage(&decode::<String>(&row, "status")?)?,
        attempts: u32::try_from(attempts).map_err(|_| {
            DurableJobError::store_message("persisted attempt count was out of range")
        })?,
        max_attempts: u32::try_from(max_attempts).map_err(|_| {
            DurableJobError::store_message("persisted max attempt count was out of range")
        })?,
        available_at_ms: decode(&row, "available_at_ms")?,
        lease_owner: decode(&row, "lease_owner")?,
        lease_until_ms: decode(&row, "lease_until_ms")?,
        idempotency_key: decode(&row, "idempotency_key")?,
        correlation_id: decode(&row, "correlation_id")?,
        last_error: decode_optional_text(&row, "last_error")?,
        created_at_ms: decode(&row, "created_at_ms")?,
        updated_at_ms: decode(&row, "updated_at_ms")?,
    })
}

fn decode_text(row: &sqlx::any::AnyRow, column: &'static str) -> DurableResult<String> {
    match row.try_get::<String, _>(column) {
        Ok(value) => Ok(value),
        Err(_) => {
            let value: Vec<u8> = row
                .try_get(column)
                .map_err(|error| DurableJobError::store("decode durable job text", error))?;
            String::from_utf8(value)
                .map_err(|error| DurableJobError::store("decode durable job UTF-8", error))
        }
    }
}

fn decode_optional_text(
    row: &sqlx::any::AnyRow,
    column: &'static str,
) -> DurableResult<Option<String>> {
    match row.try_get::<Option<String>, _>(column) {
        Ok(value) => Ok(value),
        Err(_) => {
            let value: Option<Vec<u8>> = row.try_get(column).map_err(|error| {
                DurableJobError::store("decode optional durable job text", error)
            })?;
            value
                .map(String::from_utf8)
                .transpose()
                .map_err(|error| DurableJobError::store("decode durable job UTF-8", error))
        }
    }
}

fn decode<'row, T>(row: &'row sqlx::any::AnyRow, column: &'static str) -> DurableResult<T>
where
    T: sqlx::Decode<'row, sqlx::Any> + sqlx::Type<sqlx::Any>,
{
    row.try_get(column)
        .map_err(|error| DurableJobError::store("decode durable job row", error))
}

fn unix_timestamp_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn has_verify_full(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if !matches!(url.scheme(), "postgres" | "postgresql") {
        return false;
    }
    let values = url
        .query_pairs()
        .filter(|(key, _)| key.eq_ignore_ascii_case("sslmode"))
        .map(|(_, value)| value.into_owned())
        .collect::<Vec<_>>();
    values.len() == 1 && values[0].eq_ignore_ascii_case("verify-full")
}

fn has_mysql_verify_identity(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if url.scheme() != "mysql" {
        return false;
    }
    let values = url
        .query_pairs()
        .filter(|(key, _)| key.eq_ignore_ascii_case("ssl-mode"))
        .map(|(_, value)| value.into_owned())
        .collect::<Vec<_>>();
    values.len() == 1
        && (values[0].eq_ignore_ascii_case("verify_identity")
            || values[0].eq_ignore_ascii_case("verify-identity"))
}

fn is_loopback_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .is_some_and(|url| match url.host() {
            Some(url::Host::Domain(host)) => {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
            }
            Some(url::Host::Ipv4(host)) => host.is_loopback(),
            Some(url::Host::Ipv6(host)) => host.is_loopback(),
            None => false,
        })
}

fn has_dialect_scheme(dialect: SqlDialect, value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    match dialect {
        SqlDialect::Sqlite => url.scheme() == "sqlite",
        SqlDialect::Postgres | SqlDialect::Cockroach => {
            matches!(url.scheme(), "postgres" | "postgresql")
        }
        SqlDialect::MySql => url.scheme() == "mysql",
    }
}

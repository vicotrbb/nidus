#![deny(missing_docs)]

//! First-party Redis adapter for Nidus applications.
//!
//! The provider exposes both [`redis::Client`] and the reconnecting
//! [`redis::aio::ConnectionManager`]. Nidus-owned convenience operations are
//! bounded and instrumented; raw Redis commands remain fully available and
//! retain native Redis semantics.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nidus_core::{Container, LifecycleHook, NidusError};
use nidus_integrations::{IntegrationEvent, IntegrationStatus, IntegrationTelemetry};
use thiserror::Error;
use tokio::sync::{Semaphore, SemaphorePermit};

const DEFAULT_CONCURRENCY_LIMIT: usize = 256;
const MAX_CONCURRENCY_LIMIT: usize = 65_536;

/// Result type used by Redis adapter operations.
pub type Result<T> = std::result::Result<T, RedisError>;

/// Error returned by Redis adapter operations.
#[derive(Debug, Error)]
pub enum RedisError {
    /// The Redis client or server returned an error.
    #[error(transparent)]
    Redis(#[from] redis::RedisError),
    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),
    /// Nidus config deserialization failed.
    #[cfg(feature = "nidus-config")]
    #[error(transparent)]
    Config(#[from] nidus_config::ConfigError),
    /// A required bound was configured as zero.
    #[error("{field} must be greater than zero")]
    InvalidBound {
        /// Invalid configuration field.
        field: &'static str,
    },
    /// The connection URL is not safe for the selected environment.
    #[error("invalid Redis configuration: {message}")]
    Configuration {
        /// Redaction-safe validation message.
        message: &'static str,
    },
    /// The provider stopped accepting adapter-owned work during shutdown.
    #[error("Redis provider is shutting down")]
    ShuttingDown,
    /// Admitted Redis operations did not drain before the shutdown deadline.
    #[error("Redis operations did not drain before the shutdown timeout")]
    ShutdownTimeout,
}

/// Typed Redis connection and reconnect configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct RedisConfig {
    url: String,
    connection_timeout: Duration,
    response_timeout: Duration,
    shutdown_timeout: Duration,
    reconnect_attempts: usize,
    reconnect_min_delay: Duration,
    reconnect_max_delay: Duration,
    concurrency_limit: usize,
    pipeline_buffer_size: usize,
    allow_plaintext_local: bool,
}

impl RedisConfig {
    /// Creates secure, bounded Redis configuration from an explicit URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            connection_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(2),
            shutdown_timeout: Duration::from_secs(5),
            reconnect_attempts: 6,
            reconnect_min_delay: Duration::from_millis(100),
            reconnect_max_delay: Duration::from_secs(5),
            concurrency_limit: DEFAULT_CONCURRENCY_LIMIT,
            pipeline_buffer_size: 1024,
            allow_plaintext_local: false,
        }
    }

    /// Sets the timeout for initial and reconnect attempts.
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Sets the timeout for command responses.
    pub fn with_response_timeout(mut self, timeout: Duration) -> Self {
        self.response_timeout = timeout;
        self
    }

    /// Sets how long graceful shutdown waits for admitted operations.
    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    /// Sets the maximum reconnect attempts performed by redis-rs.
    pub fn with_reconnect_attempts(mut self, attempts: usize) -> Self {
        self.reconnect_attempts = attempts;
        self
    }

    /// Sets minimum and maximum exponential reconnect delays.
    pub fn with_reconnect_backoff(mut self, minimum: Duration, maximum: Duration) -> Self {
        self.reconnect_min_delay = minimum;
        self.reconnect_max_delay = maximum;
        self
    }

    /// Sets the maximum concurrent requests accepted by the connection manager.
    pub fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = limit;
        self
    }

    /// Sets the bounded internal pipeline buffer size.
    pub fn with_pipeline_buffer_size(mut self, size: usize) -> Self {
        self.pipeline_buffer_size = size;
        self
    }

    /// Explicitly permits `redis://` only for a loopback development server.
    pub fn allow_plaintext_for_local_development(mut self) -> Self {
        self.allow_plaintext_local = true;
        self
    }

    /// Returns the configured Redis URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Validates all non-zero bounds before connecting.
    pub fn validate(&self) -> Result<()> {
        for (field, value) in [
            ("connection_timeout", self.connection_timeout.as_nanos()),
            ("response_timeout", self.response_timeout.as_nanos()),
            ("shutdown_timeout", self.shutdown_timeout.as_nanos()),
            ("reconnect_min_delay", self.reconnect_min_delay.as_nanos()),
            ("reconnect_max_delay", self.reconnect_max_delay.as_nanos()),
            ("concurrency_limit", self.concurrency_limit as u128),
            ("pipeline_buffer_size", self.pipeline_buffer_size as u128),
        ] {
            if value == 0 {
                return Err(RedisError::InvalidBound { field });
            }
        }
        if self.concurrency_limit > MAX_CONCURRENCY_LIMIT {
            return Err(RedisError::Configuration {
                message: "Redis concurrency_limit must not exceed 65536",
            });
        }
        if self.reconnect_max_delay < self.reconnect_min_delay {
            return Err(RedisError::Configuration {
                message: "Redis reconnect delays must be ordered",
            });
        }
        if self.url.starts_with("redis://") {
            if !self.allow_plaintext_local || !redis_url_is_loopback(&self.url) {
                return Err(RedisError::Configuration {
                    message: "plaintext Redis is restricted to explicit loopback development URLs",
                });
            }
        } else if !self.url.starts_with("rediss://") && !self.url.starts_with("redis+unix://") {
            return Err(RedisError::Configuration {
                message: "production Redis URLs must use rediss:// or a Unix socket",
            });
        }
        Ok(())
    }

    /// Loads Redis config from a nested `nidus_config::Config` path.
    #[cfg(feature = "nidus-config")]
    pub fn from_config_path<I, S>(config: &nidus_config::Config, path: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        #[derive(serde::Deserialize)]
        struct RawConfig {
            url: String,
            connection_timeout_ms: Option<u64>,
            response_timeout_ms: Option<u64>,
            shutdown_timeout_ms: Option<u64>,
            reconnect_attempts: Option<usize>,
            concurrency_limit: Option<usize>,
            pipeline_buffer_size: Option<usize>,
            allow_plaintext_local: Option<bool>,
        }

        let raw: RawConfig = config.get_required_path_typed(path)?;
        let mut settings = Self::new(raw.url);
        if let Some(value) = raw.connection_timeout_ms {
            settings = settings.with_connection_timeout(Duration::from_millis(value));
        }
        if let Some(value) = raw.response_timeout_ms {
            settings = settings.with_response_timeout(Duration::from_millis(value));
        }
        if let Some(value) = raw.shutdown_timeout_ms {
            settings = settings.with_shutdown_timeout(Duration::from_millis(value));
        }
        if let Some(value) = raw.reconnect_attempts {
            settings = settings.with_reconnect_attempts(value);
        }
        if let Some(value) = raw.concurrency_limit {
            settings = settings.with_concurrency_limit(value);
        }
        if let Some(value) = raw.pipeline_buffer_size {
            settings = settings.with_pipeline_buffer_size(value);
        }
        if raw.allow_plaintext_local == Some(true) {
            settings = settings.allow_plaintext_for_local_development();
        }
        settings.validate()?;
        Ok(settings)
    }
}

impl fmt::Debug for RedisConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisConfig")
            .field("url", &"<redacted>")
            .field("connection_timeout", &self.connection_timeout)
            .field("response_timeout", &self.response_timeout)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .field("reconnect_attempts", &self.reconnect_attempts)
            .field("reconnect_min_delay", &self.reconnect_min_delay)
            .field("reconnect_max_delay", &self.reconnect_max_delay)
            .field("concurrency_limit", &self.concurrency_limit)
            .field("pipeline_buffer_size", &self.pipeline_buffer_size)
            .field("allow_plaintext_local", &self.allow_plaintext_local)
            .finish()
    }
}

fn redis_url_is_loopback(url: &str) -> bool {
    url::Url::parse(url).ok().is_some_and(|url| {
        url.scheme() == "redis"
            && match url.host() {
                Some(url::Host::Domain(host)) => {
                    host.eq_ignore_ascii_case("localhost")
                        || host
                            .parse::<std::net::IpAddr>()
                            .is_ok_and(|address| address.is_loopback())
                }
                Some(url::Host::Ipv4(host)) => host.is_loopback(),
                Some(url::Host::Ipv6(host)) => host.is_loopback(),
                None => false,
            }
    })
}

/// Builder for a reconnecting Redis provider.
#[derive(Clone, Debug)]
pub struct RedisProviderBuilder {
    config: RedisConfig,
    telemetry: IntegrationTelemetry,
}

impl RedisProviderBuilder {
    /// Creates a builder from an explicit Redis URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            config: RedisConfig::new(url),
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Replaces the typed Redis configuration.
    pub fn config(mut self, config: RedisConfig) -> Self {
        self.config = config;
        self
    }

    /// Adds shared tracing, metrics, dashboard, or custom telemetry.
    pub fn telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Builds the raw Redis client without performing network I/O.
    pub fn build_client(&self) -> Result<redis::Client> {
        self.config.validate()?;
        Ok(redis::Client::open(self.config.url.as_str())?)
    }

    /// Connects and returns a provider wrapping native Redis clients.
    pub async fn connect(self) -> Result<RedisProvider> {
        self.config.validate()?;
        let started_at = Instant::now();
        let client = redis::Client::open(self.config.url.as_str())?;
        let manager_config = redis::aio::ConnectionManagerConfig::new()
            .set_number_of_retries(self.config.reconnect_attempts)
            .set_min_delay(self.config.reconnect_min_delay)
            .set_max_delay(self.config.reconnect_max_delay)
            .set_response_timeout(Some(self.config.response_timeout))
            .set_connection_timeout(Some(self.config.connection_timeout))
            .set_concurrency_limit(self.config.concurrency_limit)
            .set_pipeline_buffer_size(self.config.pipeline_buffer_size);
        let result =
            redis::aio::ConnectionManager::new_with_config(client.clone(), manager_config).await;
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-redis",
                "connect",
                if result.is_ok() {
                    IntegrationStatus::Success
                } else {
                    IntegrationStatus::Failure
                },
                started_at.elapsed(),
            ))
            .await;
        Ok(RedisProvider {
            client,
            connection: result?,
            in_flight: Arc::new(Semaphore::new(self.config.concurrency_limit)),
            max_in_flight: self.config.concurrency_limit as u32,
            shutdown_timeout: self.config.shutdown_timeout,
            shutting_down: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            telemetry: self.telemetry,
        })
    }

    /// Connects and registers the provider as a Nidus singleton.
    pub async fn register(self, container: &mut Container) -> Result<()> {
        container.register_singleton(self.connect().await?)?;
        Ok(())
    }
}

/// Nidus provider exposing native Redis clients and bounded conveniences.
#[derive(Clone)]
pub struct RedisProvider {
    client: redis::Client,
    connection: redis::aio::ConnectionManager,
    in_flight: Arc<Semaphore>,
    max_in_flight: u32,
    shutdown_timeout: Duration,
    shutting_down: Arc<AtomicBool>,
    shutdown_complete: Arc<AtomicBool>,
    telemetry: IntegrationTelemetry,
}

impl fmt::Debug for RedisProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisProvider")
            .field("client", &"redis::Client(<redacted>)")
            .field("connection", &"redis::aio::ConnectionManager")
            .field("shutting_down", &self.shutting_down.load(Ordering::Acquire))
            .field("available_permits", &self.in_flight.available_permits())
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

impl RedisProvider {
    /// Creates a Redis provider builder.
    pub fn builder(url: impl Into<String>) -> RedisProviderBuilder {
        RedisProviderBuilder::new(url)
    }

    /// Creates a provider from existing native Redis clients.
    pub fn from_parts(client: redis::Client, connection: redis::aio::ConnectionManager) -> Self {
        Self {
            client,
            connection,
            in_flight: Arc::new(Semaphore::new(DEFAULT_CONCURRENCY_LIMIT)),
            max_in_flight: DEFAULT_CONCURRENCY_LIMIT as u32,
            shutdown_timeout: Duration::from_secs(5),
            shutting_down: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Returns the native Redis client.
    pub fn client(&self) -> &redis::Client {
        &self.client
    }

    /// Returns the reconnecting, cloneable native connection manager.
    pub fn connection_manager(&self) -> &redis::aio::ConnectionManager {
        &self.connection
    }

    /// Returns a clone of the reconnecting native connection manager.
    pub fn connection(&self) -> redis::aio::ConnectionManager {
        self.connection.clone()
    }

    /// Gets a binary value using a Nidus-observed Redis operation.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let mut connection = self.connection.clone();
        let result = redis::cmd("GET")
            .arg(key)
            .query_async(&mut connection)
            .await;
        self.record("get", result.is_ok(), started_at).await;
        Ok(result?)
    }

    /// Sets a binary value, optionally with a positive TTL.
    pub async fn set(&self, key: &str, value: &[u8], ttl: Option<Duration>) -> Result<()> {
        if let Some(ttl) = ttl
            && ttl.is_zero()
        {
            return Err(RedisError::InvalidBound { field: "ttl" });
        }
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let mut command = redis::cmd("SET");
        command.arg(key).arg(value);
        if let Some(ttl) = ttl {
            command
                .arg("PX")
                .arg(ttl.as_millis().min(u128::from(u64::MAX)) as u64);
        }
        let mut connection = self.connection.clone();
        let result: redis::RedisResult<()> = command.query_async(&mut connection).await;
        self.record("set", result.is_ok(), started_at).await;
        Ok(result?)
    }

    /// Deletes one key and returns whether it existed.
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let mut connection = self.connection.clone();
        let result: redis::RedisResult<u64> = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut connection)
            .await;
        self.record("delete", result.is_ok(), started_at).await;
        Ok(result? > 0)
    }

    /// Executes a lightweight Redis `PING` readiness check.
    #[cfg(feature = "health")]
    pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
        let Ok(_permit) = self.acquire_permit().await else {
            return nidus_http::health::HealthStatus::down("redis provider is shutting down");
        };
        let started_at = Instant::now();
        let mut connection = self.connection.clone();
        let result: redis::RedisResult<String> =
            redis::cmd("PING").query_async(&mut connection).await;
        self.record("health", result.as_deref() == Ok("PONG"), started_at)
            .await;
        if result.as_deref() == Ok("PONG") {
            nidus_http::health::HealthStatus::up()
        } else {
            nidus_http::health::HealthStatus::down("redis readiness check failed")
        }
    }

    /// Adds this provider as a readiness check on a health registry.
    #[cfg(feature = "health")]
    pub fn register_ready_check(
        self: std::sync::Arc<Self>,
        registry: nidus_http::health::HealthRegistry,
        name: impl Into<String>,
    ) -> nidus_http::health::HealthRegistry {
        registry.ready_check(name, move || {
            let provider = std::sync::Arc::clone(&self);
            async move { provider.health_status().await }
        })
    }

    /// Stops new adapter-owned work and waits for admitted operations to drain.
    ///
    /// Existing native client clones remain under application ownership.
    pub async fn shutdown(&self) -> Result<()> {
        if self.shutdown_complete.load(Ordering::Acquire) {
            return Ok(());
        }
        self.shutting_down.store(true, Ordering::Release);
        let drained = tokio::time::timeout(
            self.shutdown_timeout,
            self.in_flight.acquire_many(self.max_in_flight),
        )
        .await
        .map_err(|_| RedisError::ShutdownTimeout)?
        .map_err(|_| RedisError::ShuttingDown)?;
        self.shutdown_complete.store(true, Ordering::Release);
        self.in_flight.close();
        drop(drained);
        Ok(())
    }

    async fn acquire_permit(&self) -> Result<SemaphorePermit<'_>> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(RedisError::ShuttingDown);
        }
        let permit = self
            .in_flight
            .acquire()
            .await
            .map_err(|_| RedisError::ShuttingDown)?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(RedisError::ShuttingDown);
        }
        Ok(permit)
    }

    async fn record(&self, operation: &'static str, success: bool, started_at: Instant) {
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-redis",
                operation,
                if success {
                    IntegrationStatus::Success
                } else {
                    IntegrationStatus::Failure
                },
                started_at.elapsed(),
            ))
            .await;
    }
}

#[async_trait]
impl LifecycleHook for RedisProvider {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "Redis operations failed to drain during shutdown".to_owned(),
            })
    }
}

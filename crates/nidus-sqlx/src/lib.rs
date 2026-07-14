#![deny(missing_docs)]

//! Official SQLx adapter for Nidus applications.
//!
//! This crate is installed separately from the core `nidus` facade so SQLx
//! dependencies are only compiled by applications that choose this adapter.

use nidus_core::NidusError;
use thiserror::Error;

/// Result type used by SQLx adapter operations.
pub type Result<T> = std::result::Result<T, SqlxError>;

/// Error returned by SQLx adapter operations.
#[derive(Debug, Error)]
pub enum SqlxError {
    /// SQLx returned an error while building or checking a pool.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),

    /// Nidus config deserialization failed.
    #[cfg(feature = "nidus-config")]
    #[error(transparent)]
    Config(#[from] nidus_config::ConfigError),
}

#[cfg(any(feature = "mysql", feature = "cockroach"))]
fn configuration_error(message: &'static str) -> SqlxError {
    sqlx::Error::InvalidArgument(format!("invalid SQLx adapter configuration: {message}")).into()
}

#[cfg(feature = "sqlite")]
mod sqlite {
    #[cfg(feature = "observability")]
    use std::time::Instant;

    use super::Result;
    use nidus_core::Container;

    /// Typed configuration for a SQLx SQLite pool.
    #[derive(Clone, Eq, PartialEq)]
    pub struct SqlitePoolConfig {
        database_url: String,
        max_connections: Option<u32>,
    }

    impl SqlitePoolConfig {
        /// Creates SQLite pool config from an explicit database URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                database_url: database_url.into(),
                max_connections: None,
            }
        }

        /// Sets the maximum number of pool connections.
        pub fn with_max_connections(mut self, max_connections: u32) -> Self {
            self.max_connections = Some(max_connections);
            self
        }

        /// Returns the configured database URL.
        pub fn database_url(&self) -> &str {
            &self.database_url
        }

        /// Returns the configured maximum connection count.
        pub fn max_connections(&self) -> Option<u32> {
            self.max_connections
        }

        /// Loads SQLite pool config from a nested `nidus_config::Config` path.
        #[cfg(feature = "nidus-config")]
        pub fn from_config_path<I, S>(config: &nidus_config::Config, path: I) -> Result<Self>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            #[derive(serde::Deserialize)]
            struct RawConfig {
                url: String,
                max_connections: Option<u32>,
            }

            let raw: RawConfig = config.get_required_path_typed(path)?;
            let mut settings = Self::new(raw.url);
            if let Some(max_connections) = raw.max_connections {
                settings = settings.with_max_connections(max_connections);
            }
            Ok(settings)
        }
    }

    impl std::fmt::Debug for SqlitePoolConfig {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("SqlitePoolConfig")
                .field("database_url", &"<redacted>")
                .field("max_connections", &self.max_connections)
                .finish()
        }
    }

    /// Builder for a SQLx SQLite pool provider.
    #[derive(Clone, Debug)]
    pub struct SqlitePoolBuilder {
        config: SqlitePoolConfig,
        #[cfg(feature = "observability")]
        observer: Option<nidus_observability::ObservabilityAdapterObserver>,
    }

    impl SqlitePoolBuilder {
        /// Creates a builder using `sqlite::memory:`.
        pub fn new() -> Self {
            Self {
                config: SqlitePoolConfig::new("sqlite::memory:"),
                #[cfg(feature = "observability")]
                observer: None,
            }
        }

        /// Replaces the builder config.
        pub fn config(mut self, config: SqlitePoolConfig) -> Self {
            self.config = config;
            self
        }

        /// Sets the database URL.
        pub fn database_url(mut self, database_url: impl Into<String>) -> Self {
            self.config.database_url = database_url.into();
            self
        }

        /// Sets the maximum number of pool connections.
        pub fn max_connections(mut self, max_connections: u32) -> Self {
            self.config.max_connections = Some(max_connections);
            self
        }

        /// Instruments adapter-owned SQLx pool operations with Nidus observability.
        #[cfg(feature = "observability")]
        pub fn observability(
            mut self,
            observer: nidus_observability::ObservabilityAdapterObserver,
        ) -> Self {
            self.observer = Some(observer);
            self
        }

        /// Connects and returns a provider wrapping the real SQLx pool.
        pub async fn connect(self) -> Result<SqlitePoolProvider> {
            #[cfg(feature = "observability")]
            let observer = self.observer;
            let mut options = sqlx::sqlite::SqlitePoolOptions::new();
            if let Some(max_connections) = self.config.max_connections {
                options = options.max_connections(max_connections);
            }
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            let pool = options.connect(&self.config.database_url).await;
            #[cfg(feature = "observability")]
            record_adapter_operation(
                &observer,
                "connect",
                nidus_observability::OperationStatus::from(pool.is_ok()),
                started_at,
            );
            let pool = pool?;
            Ok(SqlitePoolProvider {
                pool,
                #[cfg(feature = "observability")]
                observer,
            })
        }

        /// Connects a provider and registers it as a Nidus singleton.
        pub async fn register(self, container: &mut Container) -> Result<()> {
            let provider = self.connect().await?;
            container.register_singleton(provider)?;
            Ok(())
        }
    }

    impl Default for SqlitePoolBuilder {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Nidus provider wrapping a real SQLx SQLite pool.
    #[derive(Clone, Debug)]
    pub struct SqlitePoolProvider {
        pool: sqlx::SqlitePool,
        #[cfg(feature = "observability")]
        observer: Option<nidus_observability::ObservabilityAdapterObserver>,
    }

    impl SqlitePoolProvider {
        /// Creates a SQLite provider builder.
        pub fn builder() -> SqlitePoolBuilder {
            SqlitePoolBuilder::new()
        }

        /// Creates a provider from an existing SQLx SQLite pool.
        pub fn from_pool(pool: sqlx::SqlitePool) -> Self {
            Self {
                pool,
                #[cfg(feature = "observability")]
                observer: None,
            }
        }

        /// Returns direct access to the underlying SQLx pool.
        pub fn pool(&self) -> &sqlx::SqlitePool {
            &self.pool
        }

        /// Consumes the provider and returns the underlying SQLx pool.
        pub fn into_pool(self) -> sqlx::SqlitePool {
            self.pool
        }

        /// Executes a lightweight readiness query.
        #[cfg(feature = "health")]
        pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            let result = sqlx::query("SELECT 1").execute(&self.pool).await;
            #[cfg(feature = "observability")]
            record_adapter_operation(
                &self.observer,
                "health",
                nidus_observability::OperationStatus::from(result.is_ok()),
                started_at,
            );
            match result {
                Ok(_) => nidus_http::health::HealthStatus::up(),
                Err(_) => nidus_http::health::HealthStatus::down("sqlite readiness check failed"),
            }
        }

        /// Adds this provider as a readiness check on a health registry.
        ///
        /// The provider is expected to be the shared instance resolved from the
        /// Nidus container, so the method takes `Arc<Self>` and does not clone
        /// the underlying SQLx pool directly.
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
    }

    #[async_trait::async_trait]
    impl nidus_core::LifecycleHook for SqlitePoolProvider {
        async fn on_shutdown(&self) -> nidus_core::Result<()> {
            self.pool.close().await;
            Ok(())
        }
    }

    #[cfg(feature = "observability")]
    fn record_adapter_operation(
        observer: &Option<nidus_observability::ObservabilityAdapterObserver>,
        operation: &'static str,
        status: nidus_observability::OperationStatus,
        started_at: Instant,
    ) {
        if let Some(observer) = observer {
            observer.record("nidus-sqlx", operation, status, started_at.elapsed());
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite::{SqlitePoolBuilder, SqlitePoolConfig, SqlitePoolProvider};

#[cfg(feature = "postgres")]
mod postgres {
    #[cfg(feature = "observability")]
    use std::time::Instant;

    use super::Result;
    use nidus_core::Container;

    /// Typed configuration for a SQLx Postgres pool.
    #[derive(Clone, Eq, PartialEq)]
    pub struct PostgresPoolConfig {
        database_url: String,
        max_connections: Option<u32>,
        min_connections: Option<u32>,
    }

    impl PostgresPoolConfig {
        /// Creates Postgres pool config from an explicit database URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                database_url: database_url.into(),
                max_connections: None,
                min_connections: None,
            }
        }

        /// Sets the maximum number of pool connections.
        pub fn with_max_connections(mut self, max_connections: u32) -> Self {
            self.max_connections = Some(max_connections);
            self
        }

        /// Sets the minimum number of pool connections.
        pub fn with_min_connections(mut self, min_connections: u32) -> Self {
            self.min_connections = Some(min_connections);
            self
        }

        /// Returns the configured database URL.
        pub fn database_url(&self) -> &str {
            &self.database_url
        }

        /// Returns the configured maximum connection count.
        pub fn max_connections(&self) -> Option<u32> {
            self.max_connections
        }

        /// Returns the configured minimum connection count.
        pub fn min_connections(&self) -> Option<u32> {
            self.min_connections
        }

        /// Loads Postgres pool config from a nested `nidus_config::Config` path.
        #[cfg(feature = "nidus-config")]
        pub fn from_config_path<I, S>(config: &nidus_config::Config, path: I) -> Result<Self>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            #[derive(serde::Deserialize)]
            struct RawConfig {
                url: String,
                max_connections: Option<u32>,
                min_connections: Option<u32>,
            }

            let raw: RawConfig = config.get_required_path_typed(path)?;
            let mut settings = Self::new(raw.url);
            if let Some(max_connections) = raw.max_connections {
                settings = settings.with_max_connections(max_connections);
            }
            if let Some(min_connections) = raw.min_connections {
                settings = settings.with_min_connections(min_connections);
            }
            Ok(settings)
        }
    }

    impl std::fmt::Debug for PostgresPoolConfig {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("PostgresPoolConfig")
                .field("database_url", &"<redacted>")
                .field("max_connections", &self.max_connections)
                .field("min_connections", &self.min_connections)
                .finish()
        }
    }

    /// Builder for a SQLx Postgres pool provider.
    #[derive(Clone, Debug)]
    pub struct PostgresPoolBuilder {
        config: PostgresPoolConfig,
        #[cfg(feature = "observability")]
        observer: Option<nidus_observability::ObservabilityAdapterObserver>,
    }

    impl PostgresPoolBuilder {
        /// Creates a builder using an explicit database URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                config: PostgresPoolConfig::new(database_url),
                #[cfg(feature = "observability")]
                observer: None,
            }
        }

        /// Replaces the builder config.
        pub fn config(mut self, config: PostgresPoolConfig) -> Self {
            self.config = config;
            self
        }

        /// Sets the database URL.
        pub fn database_url(mut self, database_url: impl Into<String>) -> Self {
            self.config.database_url = database_url.into();
            self
        }

        /// Sets the maximum number of pool connections.
        pub fn max_connections(mut self, max_connections: u32) -> Self {
            self.config.max_connections = Some(max_connections);
            self
        }

        /// Sets the minimum number of pool connections.
        pub fn min_connections(mut self, min_connections: u32) -> Self {
            self.config.min_connections = Some(min_connections);
            self
        }

        /// Instruments adapter-owned SQLx pool operations with Nidus observability.
        #[cfg(feature = "observability")]
        pub fn observability(
            mut self,
            observer: nidus_observability::ObservabilityAdapterObserver,
        ) -> Self {
            self.observer = Some(observer);
            self
        }

        /// Connects and returns a provider wrapping the real SQLx pool.
        pub async fn connect(self) -> Result<PostgresPoolProvider> {
            #[cfg(feature = "observability")]
            let observer = self.observer;
            let mut options = sqlx::postgres::PgPoolOptions::new();
            if let Some(max_connections) = self.config.max_connections {
                options = options.max_connections(max_connections);
            }
            if let Some(min_connections) = self.config.min_connections {
                options = options.min_connections(min_connections);
            }
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            let pool = options.connect(&self.config.database_url).await;
            #[cfg(feature = "observability")]
            record_adapter_operation(
                &observer,
                "connect",
                nidus_observability::OperationStatus::from(pool.is_ok()),
                started_at,
            );
            let pool = pool?;
            Ok(PostgresPoolProvider {
                pool,
                #[cfg(feature = "observability")]
                observer,
            })
        }

        /// Connects a provider and registers it as a Nidus singleton.
        pub async fn register(self, container: &mut Container) -> Result<()> {
            let provider = self.connect().await?;
            container.register_singleton(provider)?;
            Ok(())
        }
    }

    /// Nidus provider wrapping a real SQLx Postgres pool.
    #[derive(Clone, Debug)]
    pub struct PostgresPoolProvider {
        pool: sqlx::PgPool,
        #[cfg(feature = "observability")]
        observer: Option<nidus_observability::ObservabilityAdapterObserver>,
    }

    impl PostgresPoolProvider {
        /// Creates a Postgres provider builder.
        pub fn builder(database_url: impl Into<String>) -> PostgresPoolBuilder {
            PostgresPoolBuilder::new(database_url)
        }

        /// Creates a provider from an existing SQLx Postgres pool.
        pub fn from_pool(pool: sqlx::PgPool) -> Self {
            Self {
                pool,
                #[cfg(feature = "observability")]
                observer: None,
            }
        }

        /// Returns direct access to the underlying SQLx pool.
        pub fn pool(&self) -> &sqlx::PgPool {
            &self.pool
        }

        /// Consumes the provider and returns the underlying SQLx pool.
        pub fn into_pool(self) -> sqlx::PgPool {
            self.pool
        }

        /// Executes a lightweight readiness query.
        #[cfg(feature = "health")]
        pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            let result = sqlx::query("SELECT 1").execute(&self.pool).await;
            #[cfg(feature = "observability")]
            record_adapter_operation(
                &self.observer,
                "health",
                nidus_observability::OperationStatus::from(result.is_ok()),
                started_at,
            );
            match result {
                Ok(_) => nidus_http::health::HealthStatus::up(),
                Err(_) => nidus_http::health::HealthStatus::down("postgres readiness check failed"),
            }
        }

        /// Adds this provider as a readiness check on a health registry.
        ///
        /// The provider is expected to be the shared instance resolved from the
        /// Nidus container, so the method takes `Arc<Self>` and does not clone
        /// the underlying SQLx pool directly.
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
    }

    #[async_trait::async_trait]
    impl nidus_core::LifecycleHook for PostgresPoolProvider {
        async fn on_shutdown(&self) -> nidus_core::Result<()> {
            self.pool.close().await;
            Ok(())
        }
    }

    #[cfg(feature = "observability")]
    fn record_adapter_operation(
        observer: &Option<nidus_observability::ObservabilityAdapterObserver>,
        operation: &'static str,
        status: nidus_observability::OperationStatus,
        started_at: Instant,
    ) {
        if let Some(observer) = observer {
            observer.record("nidus-sqlx", operation, status, started_at.elapsed());
        }
    }
}

#[cfg(feature = "postgres")]
pub use postgres::{PostgresPoolBuilder, PostgresPoolConfig, PostgresPoolProvider};

#[cfg(feature = "mysql")]
mod mysql {
    use std::{str::FromStr, time::Instant};

    use nidus_core::Container;
    use nidus_integrations::{IntegrationEvent, IntegrationStatus, IntegrationTelemetry};

    use super::Result;

    /// Typed configuration for a SQLx MySQL pool.
    #[derive(Clone, Eq, PartialEq)]
    pub struct MySqlPoolConfig {
        database_url: String,
        max_connections: Option<u32>,
        min_connections: Option<u32>,
        allow_insecure_local: bool,
    }

    impl MySqlPoolConfig {
        /// Creates MySQL pool config from an explicit database URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                database_url: database_url.into(),
                max_connections: None,
                min_connections: None,
                allow_insecure_local: false,
            }
        }

        /// Sets the maximum number of pool connections.
        pub fn with_max_connections(mut self, max_connections: u32) -> Self {
            self.max_connections = Some(max_connections);
            self
        }

        /// Sets the minimum number of pool connections.
        pub fn with_min_connections(mut self, min_connections: u32) -> Self {
            self.min_connections = Some(min_connections);
            self
        }

        /// Explicitly permits non-verifying TLS only for loopback development.
        pub fn allow_insecure_for_local_development(mut self) -> Self {
            self.allow_insecure_local = true;
            self
        }

        /// Returns the configured database URL.
        pub fn database_url(&self) -> &str {
            &self.database_url
        }

        /// Returns the configured maximum connection count.
        pub fn max_connections(&self) -> Option<u32> {
            self.max_connections
        }

        /// Returns the configured minimum connection count.
        pub fn min_connections(&self) -> Option<u32> {
            self.min_connections
        }

        /// Validates pool bounds and TLS hostname verification.
        pub fn validate(&self) -> Result<()> {
            if self.max_connections == Some(0) {
                return Err(super::configuration_error(
                    "MySQL max_connections must be greater than zero",
                ));
            }
            if let (Some(minimum), Some(maximum)) = (self.min_connections, self.max_connections)
                && minimum > maximum
            {
                return Err(super::configuration_error(
                    "MySQL min_connections cannot exceed max_connections",
                ));
            }
            let options = sqlx::mysql::MySqlConnectOptions::from_str(&self.database_url)?;
            let verifies_identity = matches!(
                options.get_ssl_mode(),
                sqlx::mysql::MySqlSslMode::VerifyIdentity
            );
            if !verifies_identity
                && (!self.allow_insecure_local
                    || !matches!(options.get_host(), "localhost" | "127.0.0.1" | "::1"))
            {
                return Err(super::configuration_error(
                    "MySQL requires ssl-mode=VERIFY_IDENTITY except for explicit loopback development",
                ));
            }
            Ok(())
        }

        /// Loads MySQL pool config from a nested `nidus_config::Config` path.
        #[cfg(feature = "nidus-config")]
        pub fn from_config_path<I, S>(config: &nidus_config::Config, path: I) -> Result<Self>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            #[derive(serde::Deserialize)]
            struct RawConfig {
                url: String,
                max_connections: Option<u32>,
                min_connections: Option<u32>,
                allow_insecure_local: Option<bool>,
            }

            let raw: RawConfig = config.get_required_path_typed(path)?;
            let mut settings = Self::new(raw.url);
            if let Some(value) = raw.max_connections {
                settings = settings.with_max_connections(value);
            }
            if let Some(value) = raw.min_connections {
                settings = settings.with_min_connections(value);
            }
            if raw.allow_insecure_local == Some(true) {
                settings = settings.allow_insecure_for_local_development();
            }
            settings.validate()?;
            Ok(settings)
        }
    }

    impl std::fmt::Debug for MySqlPoolConfig {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("MySqlPoolConfig")
                .field("database_url", &"<redacted>")
                .field("max_connections", &self.max_connections)
                .field("min_connections", &self.min_connections)
                .field("allow_insecure_local", &self.allow_insecure_local)
                .finish()
        }
    }

    /// Builder for a SQLx MySQL pool provider.
    #[derive(Clone, Debug)]
    pub struct MySqlPoolBuilder {
        config: MySqlPoolConfig,
        telemetry: IntegrationTelemetry,
    }

    impl MySqlPoolBuilder {
        /// Creates a builder using an explicit database URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                config: MySqlPoolConfig::new(database_url),
                telemetry: IntegrationTelemetry::new(),
            }
        }

        /// Replaces the typed pool config.
        pub fn config(mut self, config: MySqlPoolConfig) -> Self {
            self.config = config;
            self
        }

        /// Sets the database URL.
        pub fn database_url(mut self, database_url: impl Into<String>) -> Self {
            self.config.database_url = database_url.into();
            self
        }

        /// Sets the maximum number of pool connections.
        pub fn max_connections(mut self, max_connections: u32) -> Self {
            self.config.max_connections = Some(max_connections);
            self
        }

        /// Sets the minimum number of pool connections.
        pub fn min_connections(mut self, min_connections: u32) -> Self {
            self.config.min_connections = Some(min_connections);
            self
        }

        /// Adds shared tracing, metrics, dashboard, or custom telemetry.
        pub fn telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
            self.telemetry = telemetry;
            self
        }

        /// Instruments operations with an existing Nidus observability observer.
        #[cfg(feature = "observability")]
        pub fn observability(
            mut self,
            observer: nidus_observability::ObservabilityAdapterObserver,
        ) -> Self {
            self.telemetry = self.telemetry.observability(observer);
            self
        }

        /// Connects and returns a provider wrapping the native SQLx pool.
        pub async fn connect(self) -> Result<MySqlPoolProvider> {
            self.config.validate()?;
            let started_at = Instant::now();
            let mut options = sqlx::mysql::MySqlPoolOptions::new();
            if let Some(value) = self.config.max_connections {
                options = options.max_connections(value);
            }
            if let Some(value) = self.config.min_connections {
                options = options.min_connections(value);
            }
            let pool = options.connect(&self.config.database_url).await;
            self.telemetry
                .record(&IntegrationEvent::new(
                    "nidus-sqlx-mysql",
                    "connect",
                    if pool.is_ok() {
                        IntegrationStatus::Success
                    } else {
                        IntegrationStatus::Failure
                    },
                    started_at.elapsed(),
                ))
                .await;
            Ok(MySqlPoolProvider {
                pool: pool?,
                telemetry: self.telemetry,
            })
        }

        /// Connects and registers the provider as a Nidus singleton.
        pub async fn register(self, container: &mut Container) -> Result<()> {
            container.register_singleton(self.connect().await?)?;
            Ok(())
        }
    }

    /// Nidus provider wrapping a native SQLx MySQL pool.
    #[derive(Clone, Debug)]
    pub struct MySqlPoolProvider {
        pool: sqlx::MySqlPool,
        telemetry: IntegrationTelemetry,
    }

    impl MySqlPoolProvider {
        /// Creates a MySQL provider builder.
        pub fn builder(database_url: impl Into<String>) -> MySqlPoolBuilder {
            MySqlPoolBuilder::new(database_url)
        }

        /// Creates a provider from an existing SQLx MySQL pool.
        pub fn from_pool(pool: sqlx::MySqlPool) -> Self {
            Self {
                pool,
                telemetry: IntegrationTelemetry::new(),
            }
        }

        /// Returns direct access to the native SQLx pool.
        pub fn pool(&self) -> &sqlx::MySqlPool {
            &self.pool
        }

        /// Returns the shared adapter telemetry configuration.
        pub const fn telemetry(&self) -> &IntegrationTelemetry {
            &self.telemetry
        }

        /// Consumes the provider and returns the native SQLx pool.
        pub fn into_pool(self) -> sqlx::MySqlPool {
            self.pool
        }

        /// Executes a lightweight readiness query with a safe error response.
        #[cfg(feature = "health")]
        pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
            let started_at = Instant::now();
            let result = sqlx::query("SELECT 1").execute(&self.pool).await;
            self.telemetry
                .record(&IntegrationEvent::new(
                    "nidus-sqlx-mysql",
                    "health",
                    if result.is_ok() {
                        IntegrationStatus::Success
                    } else {
                        IntegrationStatus::Failure
                    },
                    started_at.elapsed(),
                ))
                .await;
            if result.is_ok() {
                nidus_http::health::HealthStatus::up()
            } else {
                nidus_http::health::HealthStatus::down("mysql readiness check failed")
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
    }

    #[async_trait::async_trait]
    impl nidus_core::LifecycleHook for MySqlPoolProvider {
        async fn on_shutdown(&self) -> nidus_core::Result<()> {
            self.pool.close().await;
            Ok(())
        }
    }
}

#[cfg(feature = "mysql")]
pub use mysql::{MySqlPoolBuilder, MySqlPoolConfig, MySqlPoolProvider};

#[cfg(feature = "cockroach")]
mod cockroach {
    use std::{
        fmt,
        str::FromStr,
        time::{Duration, Instant},
    };

    use futures_util::future::BoxFuture;
    use nidus_core::Container;
    use nidus_integrations::{IntegrationEvent, IntegrationStatus, IntegrationTelemetry};
    use sqlx::Acquire;

    use super::Result;

    /// Result returned by a retryable CockroachDB transaction.
    pub type CockroachTransactionResult<T> = std::result::Result<T, CockroachTransactionError>;

    /// Error returned by a retryable CockroachDB transaction.
    #[derive(Debug, thiserror::Error)]
    pub enum CockroachTransactionError {
        /// SQLx returned a non-retryable error.
        #[error(transparent)]
        Sqlx(#[from] sqlx::Error),
        /// CockroachDB kept returning SQLSTATE `40001` through the attempt bound.
        #[error("CockroachDB transaction exhausted {attempts} attempts")]
        RetryExhausted {
            /// Total transaction attempts.
            attempts: usize,
            /// Last retryable SQLx error.
            #[source]
            source: sqlx::Error,
        },
    }

    /// Bounded application-level retry policy for CockroachDB serialization failures.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct CockroachRetryPolicy {
        max_attempts: usize,
        initial_backoff: Duration,
        max_backoff: Duration,
        jitter: bool,
    }

    impl CockroachRetryPolicy {
        /// Creates the default bounded policy: five attempts, 25ms to 2s backoff, full jitter.
        pub fn new() -> Self {
            Self {
                max_attempts: 5,
                initial_backoff: Duration::from_millis(25),
                max_backoff: Duration::from_secs(2),
                jitter: true,
            }
        }

        /// Sets the total number of attempts, including the first execution.
        pub fn with_max_attempts(mut self, max_attempts: usize) -> Self {
            self.max_attempts = max_attempts;
            self
        }

        /// Sets the initial and maximum exponential backoff.
        pub fn with_backoff(mut self, initial: Duration, maximum: Duration) -> Self {
            self.initial_backoff = initial;
            self.max_backoff = maximum;
            self
        }

        /// Disables jitter for deterministic tests.
        pub fn without_jitter(mut self) -> Self {
            self.jitter = false;
            self
        }

        /// Returns the total allowed transaction attempts.
        pub const fn max_attempts(&self) -> usize {
            self.max_attempts
        }

        /// Returns the maximum delay before a one-based retry attempt.
        pub fn maximum_delay_for_retry(&self, retry: usize) -> Duration {
            let shift = retry.saturating_sub(1).min(31) as u32;
            self.initial_backoff
                .saturating_mul(1_u32 << shift)
                .min(self.max_backoff)
        }

        fn validate(&self) -> Result<()> {
            if self.max_attempts == 0 {
                return Err(super::configuration_error(
                    "CockroachDB max_attempts must be greater than zero",
                ));
            }
            if self.initial_backoff.is_zero() || self.max_backoff < self.initial_backoff {
                return Err(super::configuration_error(
                    "CockroachDB retry backoff must be positive and ordered",
                ));
            }
            Ok(())
        }

        fn delay_for_retry(&self, retry: usize) -> Duration {
            let maximum = self.maximum_delay_for_retry(retry);
            if !self.jitter {
                return maximum;
            }
            let upper = maximum.as_nanos().min(u128::from(u64::MAX)) as u64;
            Duration::from_nanos(fastrand::u64(0..=upper))
        }
    }

    impl Default for CockroachRetryPolicy {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Typed CockroachDB pool and retry configuration.
    #[derive(Clone, Eq, PartialEq)]
    pub struct CockroachPoolConfig {
        database_url: String,
        max_connections: Option<u32>,
        min_connections: Option<u32>,
        acquire_timeout: Duration,
        allow_insecure_local: bool,
        retry_policy: CockroachRetryPolicy,
    }

    impl CockroachPoolConfig {
        /// Creates CockroachDB config that requires `sslmode=verify-full`.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                database_url: database_url.into(),
                max_connections: None,
                min_connections: None,
                acquire_timeout: Duration::from_secs(5),
                allow_insecure_local: false,
                retry_policy: CockroachRetryPolicy::new(),
            }
        }

        /// Sets the maximum number of pool connections.
        pub fn with_max_connections(mut self, value: u32) -> Self {
            self.max_connections = Some(value);
            self
        }

        /// Sets the minimum number of pool connections.
        pub fn with_min_connections(mut self, value: u32) -> Self {
            self.min_connections = Some(value);
            self
        }

        /// Sets the bounded pool acquisition timeout.
        pub fn with_acquire_timeout(mut self, value: Duration) -> Self {
            self.acquire_timeout = value;
            self
        }

        /// Replaces the transaction retry policy.
        pub fn with_retry_policy(mut self, value: CockroachRetryPolicy) -> Self {
            self.retry_policy = value;
            self
        }

        /// Explicitly permits non-TLS local development connections.
        ///
        /// Never enable this for deployed applications.
        pub fn allow_insecure_for_local_development(mut self) -> Self {
            self.allow_insecure_local = true;
            self
        }

        /// Returns the configured database URL.
        pub fn database_url(&self) -> &str {
            &self.database_url
        }

        /// Returns the transaction retry policy.
        pub fn retry_policy(&self) -> &CockroachRetryPolicy {
            &self.retry_policy
        }

        /// Validates TLS and retry safety before any network I/O.
        pub fn validate(&self) -> Result<()> {
            self.retry_policy.validate()?;
            if self.max_connections == Some(0) {
                return Err(super::configuration_error(
                    "CockroachDB max_connections must be greater than zero",
                ));
            }
            if self.acquire_timeout.is_zero() {
                return Err(super::configuration_error(
                    "CockroachDB acquire timeout must be greater than zero",
                ));
            }
            if let (Some(minimum), Some(maximum)) = (self.min_connections, self.max_connections)
                && minimum > maximum
            {
                return Err(super::configuration_error(
                    "CockroachDB min_connections cannot exceed max_connections",
                ));
            }
            let options = sqlx::postgres::PgConnectOptions::from_str(&self.database_url)?;
            let verify_full = matches!(
                options.get_ssl_mode(),
                sqlx::postgres::PgSslMode::VerifyFull
            );
            if !verify_full
                && (!self.allow_insecure_local
                    || !matches!(options.get_host(), "localhost" | "127.0.0.1" | "::1"))
            {
                return Err(super::configuration_error(
                    "CockroachDB requires sslmode=verify-full except for explicit loopback development",
                ));
            }
            Ok(())
        }

        /// Loads CockroachDB config from a nested `nidus_config::Config` path.
        #[cfg(feature = "nidus-config")]
        pub fn from_config_path<I, S>(config: &nidus_config::Config, path: I) -> Result<Self>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            #[derive(serde::Deserialize)]
            struct RawConfig {
                url: String,
                max_connections: Option<u32>,
                min_connections: Option<u32>,
                acquire_timeout_ms: Option<u64>,
                max_attempts: Option<usize>,
                allow_insecure_local: Option<bool>,
            }

            let raw: RawConfig = config.get_required_path_typed(path)?;
            let mut settings = Self::new(raw.url);
            if let Some(value) = raw.max_connections {
                settings = settings.with_max_connections(value);
            }
            if let Some(value) = raw.min_connections {
                settings = settings.with_min_connections(value);
            }
            if let Some(value) = raw.acquire_timeout_ms {
                settings = settings.with_acquire_timeout(Duration::from_millis(value));
            }
            if let Some(value) = raw.max_attempts {
                settings.retry_policy = settings.retry_policy.with_max_attempts(value);
            }
            if raw.allow_insecure_local == Some(true) {
                settings = settings.allow_insecure_for_local_development();
            }
            settings.validate()?;
            Ok(settings)
        }
    }

    impl fmt::Debug for CockroachPoolConfig {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("CockroachPoolConfig")
                .field("database_url", &"<redacted>")
                .field("max_connections", &self.max_connections)
                .field("min_connections", &self.min_connections)
                .field("acquire_timeout", &self.acquire_timeout)
                .field("allow_insecure_local", &self.allow_insecure_local)
                .field("retry_policy", &self.retry_policy)
                .finish()
        }
    }

    /// Builder for a CockroachDB-compatible SQLx PostgreSQL pool.
    #[derive(Clone, Debug)]
    pub struct CockroachPoolBuilder {
        config: CockroachPoolConfig,
        telemetry: IntegrationTelemetry,
    }

    impl CockroachPoolBuilder {
        /// Creates a builder from an explicit CockroachDB URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                config: CockroachPoolConfig::new(database_url),
                telemetry: IntegrationTelemetry::new(),
            }
        }

        /// Replaces the typed CockroachDB config.
        pub fn config(mut self, config: CockroachPoolConfig) -> Self {
            self.config = config;
            self
        }

        /// Adds shared tracing, metrics, dashboard, or custom telemetry.
        pub fn telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
            self.telemetry = telemetry;
            self
        }

        /// Instruments operations with an existing Nidus observability observer.
        #[cfg(feature = "observability")]
        pub fn observability(
            mut self,
            observer: nidus_observability::ObservabilityAdapterObserver,
        ) -> Self {
            self.telemetry = self.telemetry.observability(observer);
            self
        }

        /// Connects after enforcing the default `verify-full` TLS policy.
        pub async fn connect(self) -> Result<CockroachPoolProvider> {
            self.config.validate()?;
            let started_at = Instant::now();
            let connect_options =
                sqlx::postgres::PgConnectOptions::from_str(&self.config.database_url)?;
            let mut pool_options =
                sqlx::postgres::PgPoolOptions::new().acquire_timeout(self.config.acquire_timeout);
            if let Some(value) = self.config.max_connections {
                pool_options = pool_options.max_connections(value);
            }
            if let Some(value) = self.config.min_connections {
                pool_options = pool_options.min_connections(value);
            }
            let pool = pool_options.connect_with(connect_options).await;
            self.telemetry
                .record(&IntegrationEvent::new(
                    "nidus-sqlx-cockroach",
                    "connect",
                    if pool.is_ok() {
                        IntegrationStatus::Success
                    } else {
                        IntegrationStatus::Failure
                    },
                    started_at.elapsed(),
                ))
                .await;
            Ok(CockroachPoolProvider {
                pool: pool?,
                retry_policy: self.config.retry_policy,
                telemetry: self.telemetry,
            })
        }

        /// Connects and registers the provider as a Nidus singleton.
        pub async fn register(self, container: &mut Container) -> Result<()> {
            container.register_singleton(self.connect().await?)?;
            Ok(())
        }
    }

    /// Future returned by a retryable CockroachDB transaction callback.
    pub type CockroachTransactionFuture<'connection, T> =
        BoxFuture<'connection, std::result::Result<T, sqlx::Error>>;

    /// Nidus provider exposing the native SQLx PostgreSQL pool for CockroachDB.
    #[derive(Clone, Debug)]
    pub struct CockroachPoolProvider {
        pool: sqlx::PgPool,
        retry_policy: CockroachRetryPolicy,
        telemetry: IntegrationTelemetry,
    }

    impl CockroachPoolProvider {
        /// Creates a CockroachDB provider builder.
        pub fn builder(database_url: impl Into<String>) -> CockroachPoolBuilder {
            CockroachPoolBuilder::new(database_url)
        }

        /// Creates a provider from an existing SQLx pool and retry policy.
        pub fn from_pool(pool: sqlx::PgPool, retry_policy: CockroachRetryPolicy) -> Result<Self> {
            retry_policy.validate()?;
            Ok(Self {
                pool,
                retry_policy,
                telemetry: IntegrationTelemetry::new(),
            })
        }

        /// Returns direct access to the native SQLx PostgreSQL pool.
        pub fn pool(&self) -> &sqlx::PgPool {
            &self.pool
        }

        /// Returns the bounded transaction retry policy.
        pub fn retry_policy(&self) -> &CockroachRetryPolicy {
            &self.retry_policy
        }

        /// Runs a database-only transaction and retries SQLSTATE `40001` failures.
        ///
        /// The callback may execute more than once. It must contain only
        /// transactional database effects; do not perform HTTP calls, publish
        /// messages, send email, or mutate external state inside it. SQLSTATE
        /// `40003` ambiguous results are never retried automatically.
        pub async fn transaction_with_retry<T, F>(
            &self,
            mut operation: F,
        ) -> CockroachTransactionResult<T>
        where
            T: Send,
            F: for<'connection> FnMut(
                &'connection mut sqlx::PgConnection,
            ) -> CockroachTransactionFuture<'connection, T>,
        {
            let started_at = Instant::now();
            let mut connection = self.pool.acquire().await?;
            for attempt in 1..=self.retry_policy.max_attempts {
                let mut transaction = connection.begin().await?;
                let outcome = operation(&mut transaction).await;
                let final_outcome: std::result::Result<(), sqlx::Error> = match outcome {
                    Ok(value) => match transaction.commit().await {
                        Ok(()) => {
                            self.record_transaction(true, started_at).await;
                            return Ok(value);
                        }
                        Err(error) => Err(error),
                    },
                    Err(error) => {
                        let _ = transaction.rollback().await;
                        Err(error)
                    }
                };

                let error = final_outcome.unwrap_err();
                if !is_retryable_serialization_error(&error) {
                    self.record_transaction(false, started_at).await;
                    return Err(error.into());
                }
                if attempt == self.retry_policy.max_attempts {
                    self.record_transaction(false, started_at).await;
                    return Err(CockroachTransactionError::RetryExhausted {
                        attempts: attempt,
                        source: error,
                    });
                }
                tokio::time::sleep(self.retry_policy.delay_for_retry(attempt)).await;
            }
            unreachable!("validated CockroachDB retry policy always attempts at least once")
        }

        /// Executes a lightweight readiness query with a safe error response.
        #[cfg(feature = "health")]
        pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
            let started_at = Instant::now();
            let result = sqlx::query("SELECT 1").execute(&self.pool).await;
            self.telemetry
                .record(&IntegrationEvent::new(
                    "nidus-sqlx-cockroach",
                    "health",
                    if result.is_ok() {
                        IntegrationStatus::Success
                    } else {
                        IntegrationStatus::Failure
                    },
                    started_at.elapsed(),
                ))
                .await;
            if result.is_ok() {
                nidus_http::health::HealthStatus::up()
            } else {
                nidus_http::health::HealthStatus::down("cockroachdb readiness check failed")
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

        async fn record_transaction(&self, success: bool, started_at: Instant) {
            self.telemetry
                .record(&IntegrationEvent::new(
                    "nidus-sqlx-cockroach",
                    "transaction",
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

    #[async_trait::async_trait]
    impl nidus_core::LifecycleHook for CockroachPoolProvider {
        async fn on_shutdown(&self) -> nidus_core::Result<()> {
            self.pool.close().await;
            Ok(())
        }
    }

    fn is_retryable_serialization_error(error: &sqlx::Error) -> bool {
        match error {
            sqlx::Error::Database(database) => database.code().as_deref() == Some("40001"),
            _ => false,
        }
    }
}

#[cfg(feature = "cockroach")]
pub use cockroach::{
    CockroachPoolBuilder, CockroachPoolConfig, CockroachPoolProvider, CockroachRetryPolicy,
    CockroachTransactionError, CockroachTransactionFuture, CockroachTransactionResult,
};

#![deny(missing_docs)]

//! Official SQLx adapter for Nidus applications.
//!
//! This crate is installed separately from the core `nidus` facade so SQLx
//! dependencies are only compiled by applications that choose this adapter.

use nidus_core::{Container, NidusError, ProviderRegistrant, Result as NidusResult};
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

#[cfg(feature = "sqlite")]
mod sqlite {
    use super::{Container, NidusResult, ProviderRegistrant, Result};

    /// Typed configuration for a SQLx SQLite pool.
    #[derive(Clone, Debug, Eq, PartialEq)]
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

    /// Builder for a SQLx SQLite pool provider.
    #[derive(Clone, Debug)]
    pub struct SqlitePoolBuilder {
        config: SqlitePoolConfig,
    }

    impl SqlitePoolBuilder {
        /// Creates a builder using `sqlite::memory:`.
        pub fn new() -> Self {
            Self {
                config: SqlitePoolConfig::new("sqlite::memory:"),
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

        /// Connects and returns a provider wrapping the real SQLx pool.
        pub async fn connect(self) -> Result<SqlitePoolProvider> {
            let mut options = sqlx::sqlite::SqlitePoolOptions::new();
            if let Some(max_connections) = self.config.max_connections {
                options = options.max_connections(max_connections);
            }
            let pool = options.connect(&self.config.database_url).await?;
            Ok(SqlitePoolProvider { pool })
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
    }

    impl SqlitePoolProvider {
        /// Creates a SQLite provider builder.
        pub fn builder() -> SqlitePoolBuilder {
            SqlitePoolBuilder::new()
        }

        /// Creates a provider from an existing SQLx SQLite pool.
        pub fn from_pool(pool: sqlx::SqlitePool) -> Self {
            Self { pool }
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
            match sqlx::query("SELECT 1").execute(&self.pool).await {
                Ok(_) => nidus_http::health::HealthStatus::up(),
                Err(error) => nidus_http::health::HealthStatus::down(error.to_string()),
            }
        }
    }

    impl ProviderRegistrant for SqlitePoolProvider {
        fn register_provider(_container: &mut Container) -> NidusResult<()> {
            Ok(())
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite::{SqlitePoolBuilder, SqlitePoolConfig, SqlitePoolProvider};

#[cfg(feature = "postgres")]
mod postgres {
    use super::{Container, NidusResult, ProviderRegistrant, Result};

    /// Typed configuration for a SQLx Postgres pool.
    #[derive(Clone, Debug, Eq, PartialEq)]
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

    /// Builder for a SQLx Postgres pool provider.
    #[derive(Clone, Debug)]
    pub struct PostgresPoolBuilder {
        config: PostgresPoolConfig,
    }

    impl PostgresPoolBuilder {
        /// Creates a builder using an explicit database URL.
        pub fn new(database_url: impl Into<String>) -> Self {
            Self {
                config: PostgresPoolConfig::new(database_url),
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

        /// Connects and returns a provider wrapping the real SQLx pool.
        pub async fn connect(self) -> Result<PostgresPoolProvider> {
            let mut options = sqlx::postgres::PgPoolOptions::new();
            if let Some(max_connections) = self.config.max_connections {
                options = options.max_connections(max_connections);
            }
            if let Some(min_connections) = self.config.min_connections {
                options = options.min_connections(min_connections);
            }
            let pool = options.connect(&self.config.database_url).await?;
            Ok(PostgresPoolProvider { pool })
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
    }

    impl PostgresPoolProvider {
        /// Creates a Postgres provider builder.
        pub fn builder(database_url: impl Into<String>) -> PostgresPoolBuilder {
            PostgresPoolBuilder::new(database_url)
        }

        /// Creates a provider from an existing SQLx Postgres pool.
        pub fn from_pool(pool: sqlx::PgPool) -> Self {
            Self { pool }
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
            match sqlx::query("SELECT 1").execute(&self.pool).await {
                Ok(_) => nidus_http::health::HealthStatus::up(),
                Err(error) => nidus_http::health::HealthStatus::down(error.to_string()),
            }
        }
    }

    impl ProviderRegistrant for PostgresPoolProvider {
        fn register_provider(_container: &mut Container) -> NidusResult<()> {
            Ok(())
        }
    }
}

#[cfg(feature = "postgres")]
pub use postgres::{PostgresPoolBuilder, PostgresPoolConfig, PostgresPoolProvider};

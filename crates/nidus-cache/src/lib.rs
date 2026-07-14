#![deny(missing_docs)]

//! Official cache adapter for Nidus applications.
//!
//! This crate is installed separately from the core `nidus` facade so cache
//! backend dependencies are only compiled by applications that choose them.

use nidus_core::NidusError;
use thiserror::Error;

/// Result type used by cache adapter operations.
pub type Result<T> = std::result::Result<T, CacheError>;

/// Error returned by cache adapter operations.
#[derive(Debug, Error)]
pub enum CacheError {
    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),
}

/// Cache provider configuration shared by cache backends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheConfig {
    namespace: Option<String>,
    time_to_live: Option<std::time::Duration>,
    max_capacity: Option<u64>,
}

impl CacheConfig {
    /// Creates empty cache configuration.
    pub fn new() -> Self {
        Self {
            namespace: None,
            time_to_live: None,
            max_capacity: None,
        }
    }

    /// Sets the namespace prefix applied to logical cache keys.
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Sets the default time to live for cache entries.
    pub fn time_to_live(mut self, time_to_live: std::time::Duration) -> Self {
        self.time_to_live = Some(time_to_live);
        self
    }

    /// Sets the maximum weighted entry capacity.
    pub fn max_capacity(mut self, max_capacity: u64) -> Self {
        self.max_capacity = Some(max_capacity);
        self
    }

    /// Returns the configured namespace.
    pub fn namespace_value(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    /// Returns the configured default time to live.
    pub fn time_to_live_value(&self) -> Option<std::time::Duration> {
        self.time_to_live
    }

    /// Returns the configured maximum capacity.
    pub fn max_capacity_value(&self) -> Option<u64> {
        self.max_capacity
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Namespaced cache key helper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheKey(String);

impl CacheKey {
    /// Creates a cache key from optional namespace and logical key parts.
    pub fn new(namespace: Option<&str>, key: impl AsRef<str>) -> Self {
        match namespace {
            Some(namespace) if !namespace.is_empty() => Self::namespaced(namespace, key.as_ref()),
            _ => Self(key.as_ref().to_owned()),
        }
    }

    fn namespaced(namespace: &str, key: &str) -> Self {
        let mut backend_key = String::with_capacity(namespace.len() + 1 + key.len());
        backend_key.push_str(namespace);
        backend_key.push(':');
        backend_key.push_str(key);
        Self(backend_key)
    }

    /// Returns the full backend key.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the key and returns the full backend key.
    pub fn into_string(self) -> String {
        self.0
    }
}

#[cfg(feature = "moka")]
mod moka_backend {
    #[cfg(feature = "observability")]
    use std::time::Instant;

    use nidus_core::{Container, ProviderRegistrant, Result as NidusResult};

    use super::{CacheConfig, CacheKey, Result};

    /// Builder for a Moka local in-memory cache provider.
    #[derive(Clone, Debug, Default)]
    pub struct MokaCacheBuilder {
        config: CacheConfig,
        #[cfg(feature = "observability")]
        observer: Option<nidus_observability::ObservabilityAdapterObserver>,
    }

    impl MokaCacheBuilder {
        /// Creates a Moka cache builder.
        pub fn new() -> Self {
            Self::default()
        }

        /// Replaces the builder config.
        pub fn config(mut self, config: CacheConfig) -> Self {
            self.config = config;
            self
        }

        /// Sets the namespace prefix applied to logical cache keys.
        pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
            self.config = self.config.namespace(namespace);
            self
        }

        /// Sets the default time to live for cache entries.
        pub fn time_to_live(mut self, time_to_live: std::time::Duration) -> Self {
            self.config = self.config.time_to_live(time_to_live);
            self
        }

        /// Sets the maximum weighted entry capacity.
        pub fn max_capacity(mut self, max_capacity: u64) -> Self {
            self.config = self.config.max_capacity(max_capacity);
            self
        }

        /// Instruments adapter-owned cache operations with Nidus observability.
        #[cfg(feature = "observability")]
        pub fn observability(
            mut self,
            observer: nidus_observability::ObservabilityAdapterObserver,
        ) -> Self {
            self.observer = Some(observer);
            self
        }

        /// Builds a Moka cache provider.
        pub fn build(self) -> MokaCacheProvider {
            let mut builder = moka::future::Cache::builder();
            if let Some(time_to_live) = self.config.time_to_live {
                builder = builder.time_to_live(time_to_live);
            }
            if let Some(max_capacity) = self.config.max_capacity {
                builder = builder.max_capacity(max_capacity);
            }
            MokaCacheProvider {
                namespace: self.config.namespace,
                cache: builder.build(),
                #[cfg(feature = "observability")]
                observer: self.observer,
            }
        }

        /// Builds and registers a Moka cache provider as a Nidus singleton.
        pub fn register(self, container: &mut Container) -> Result<()> {
            container.register_singleton(self.build())?;
            Ok(())
        }
    }

    /// Nidus provider wrapping a Moka local in-memory cache.
    #[derive(Clone, Debug)]
    pub struct MokaCacheProvider {
        namespace: Option<String>,
        cache: moka::future::Cache<String, Vec<u8>>,
        #[cfg(feature = "observability")]
        observer: Option<nidus_observability::ObservabilityAdapterObserver>,
    }

    impl MokaCacheProvider {
        /// Creates a Moka cache provider builder.
        pub fn builder() -> MokaCacheBuilder {
            MokaCacheBuilder::new()
        }

        /// Creates a provider from an existing Moka cache and optional namespace.
        pub fn from_cache(
            cache: moka::future::Cache<String, Vec<u8>>,
            namespace: Option<String>,
        ) -> Self {
            Self {
                namespace,
                cache,
                #[cfg(feature = "observability")]
                observer: None,
            }
        }

        /// Inserts a value by logical key.
        pub async fn insert(&self, key: impl AsRef<str>, value: Vec<u8>) {
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            self.cache
                .insert(self.cache_key(key).into_string(), value)
                .await;
            #[cfg(feature = "observability")]
            self.record(
                "insert",
                nidus_observability::OperationStatus::Success,
                started_at,
            );
        }

        /// Returns a value by logical key.
        pub async fn get(&self, key: impl AsRef<str>) -> Option<Vec<u8>> {
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            let key = key.as_ref();
            let result = match self.namespace.as_deref() {
                Some(namespace) if !namespace.is_empty() => {
                    let key = CacheKey::namespaced(namespace, key);
                    self.cache.get(key.as_str()).await
                }
                _ => self.cache.get(key).await,
            };
            #[cfg(feature = "observability")]
            self.record(
                "get",
                nidus_observability::OperationStatus::Success,
                started_at,
            );
            result
        }

        /// Invalidates a value by logical key.
        pub async fn invalidate(&self, key: impl AsRef<str>) {
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            let key = key.as_ref();
            match self.namespace.as_deref() {
                Some(namespace) if !namespace.is_empty() => {
                    let key = CacheKey::namespaced(namespace, key);
                    self.cache.invalidate(key.as_str()).await;
                }
                _ => self.cache.invalidate(key).await,
            }
            #[cfg(feature = "observability")]
            self.record(
                "invalidate",
                nidus_observability::OperationStatus::Success,
                started_at,
            );
        }

        /// Returns direct access to the underlying Moka cache.
        pub fn inner(&self) -> &moka::future::Cache<String, Vec<u8>> {
            &self.cache
        }

        /// Returns the namespace used for logical keys.
        pub fn namespace(&self) -> Option<&str> {
            self.namespace.as_deref()
        }

        /// Returns a local health status for this in-memory provider.
        #[cfg(feature = "health")]
        pub fn health_status(&self) -> nidus_http::health::HealthStatus {
            #[cfg(feature = "observability")]
            let started_at = Instant::now();
            #[cfg(feature = "observability")]
            self.record(
                "health",
                nidus_observability::OperationStatus::Success,
                started_at,
            );
            nidus_http::health::HealthStatus::up()
        }

        /// Adds this provider as a readiness check on a health registry.
        ///
        /// The provider is expected to be the shared instance resolved from the
        /// Nidus container, so the method takes `Arc<Self>` and does not clone
        /// the underlying cache directly.
        #[cfg(feature = "health")]
        pub fn register_ready_check(
            self: std::sync::Arc<Self>,
            registry: nidus_http::health::HealthRegistry,
            name: impl Into<String>,
        ) -> nidus_http::health::HealthRegistry {
            registry.ready_check_sync(name, move || self.health_status())
        }

        fn cache_key(&self, key: impl AsRef<str>) -> CacheKey {
            CacheKey::new(self.namespace.as_deref(), key)
        }

        #[cfg(feature = "observability")]
        fn record(
            &self,
            operation: &'static str,
            status: nidus_observability::OperationStatus,
            started_at: Instant,
        ) {
            if let Some(observer) = &self.observer {
                observer.record("nidus-cache", operation, status, started_at.elapsed());
            }
        }
    }

    impl ProviderRegistrant for MokaCacheProvider {
        fn register_provider(container: &mut Container) -> NidusResult<()> {
            container.register_singleton(Self::builder().build())?;
            Ok(())
        }
    }
}

#[cfg(feature = "moka")]
pub use moka_backend::{MokaCacheBuilder, MokaCacheProvider};

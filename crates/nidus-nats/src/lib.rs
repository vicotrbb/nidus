#![deny(missing_docs)]

//! First-party NATS and JetStream integration for Nidus.
//!
//! Core NATS and JetStream remain distinct native capabilities. Core publish
//! followed by `flush` proves the server processed the protocol buffer; a
//! JetStream publish additionally waits for its persistence acknowledgement.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::Bytes;
use nidus_core::{Container, LifecycleHook, NidusError};
use nidus_integrations::{
    IntegrationEvent, IntegrationStatus, IntegrationTelemetry, MessageEnvelope,
};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{Semaphore, SemaphorePermit};

const MAX_IN_FLIGHT: usize = 65_536;

/// Result type for NATS adapter operations.
pub type Result<T> = std::result::Result<T, NatsError>;

/// NATS adapter error preserving native async-nats errors.
#[derive(Debug, Error)]
pub enum NatsError {
    /// async-nats returned an error.
    #[error("NATS operation failed: {source}")]
    Nats {
        /// Native async-nats source error.
        #[source]
        source: async_nats::Error,
    },
    /// Shared envelope serialization failed.
    #[error(transparent)]
    Integration(#[from] nidus_integrations::IntegrationError),
    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),
    /// Adapter configuration was unsafe or incomplete.
    #[error("invalid NATS configuration: {message}")]
    Configuration {
        /// Redaction-safe validation message.
        message: &'static str,
    },
    /// The bounded provider was closed during an operation.
    #[error("NATS provider is shutting down")]
    ShuttingDown,
    /// Operations or the native drain did not complete before the deadline.
    #[error("NATS shutdown exceeded its configured timeout")]
    ShutdownTimeout,
}

/// Typed NATS connection configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct NatsConfig {
    server: String,
    client_name: String,
    connection_timeout: Duration,
    max_reconnects: usize,
    max_in_flight: usize,
    shutdown_timeout: Duration,
    allow_plaintext_local: bool,
}

impl NatsConfig {
    /// Creates TLS-first NATS configuration with bounded reconnects and concurrency.
    pub fn new(server: impl Into<String>, client_name: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            client_name: client_name.into(),
            connection_timeout: Duration::from_secs(5),
            max_reconnects: 60,
            max_in_flight: 256,
            shutdown_timeout: Duration::from_secs(10),
            allow_plaintext_local: false,
        }
    }

    /// Explicitly permits `nats://` plaintext for local development and tests.
    pub fn allow_plaintext_for_local_development(mut self) -> Self {
        self.allow_plaintext_local = true;
        self
    }

    /// Sets the initial connection attempt timeout.
    pub fn with_connection_timeout(mut self, value: Duration) -> Self {
        self.connection_timeout = value;
        self
    }

    /// Sets the maximum reconnect attempts.
    pub fn with_max_reconnects(mut self, value: usize) -> Self {
        self.max_reconnects = value;
        self
    }

    /// Sets the maximum concurrent provider-owned publish operations.
    pub fn with_max_in_flight(mut self, value: usize) -> Self {
        self.max_in_flight = value;
        self
    }

    /// Sets the total graceful shutdown deadline.
    pub fn with_shutdown_timeout(mut self, value: Duration) -> Self {
        self.shutdown_timeout = value;
        self
    }

    /// Returns the configured server URL.
    pub fn server(&self) -> &str {
        &self.server
    }

    /// Validates TLS and bounds before network I/O.
    pub fn validate(&self) -> Result<()> {
        if self.server.trim().is_empty() || self.client_name.trim().is_empty() {
            return Err(NatsError::Configuration {
                message: "NATS server and client_name cannot be empty",
            });
        }
        if self.connection_timeout.is_zero()
            || self.shutdown_timeout.is_zero()
            || self.max_in_flight == 0
            || self.max_in_flight > MAX_IN_FLIGHT
        {
            return Err(NatsError::Configuration {
                message: "NATS timeouts and max_in_flight must be within safe bounds",
            });
        }
        if self.server.starts_with("nats://") && !self.allow_plaintext_local {
            return Err(NatsError::Configuration {
                message: "plaintext NATS requires an explicit local-development opt-in",
            });
        }
        if self.server.starts_with("nats://") && !nats_url_is_loopback(&self.server) {
            return Err(NatsError::Configuration {
                message: "plaintext NATS is restricted to loopback servers",
            });
        }
        if !self.server.starts_with("tls://")
            && !self.server.starts_with("wss://")
            && !self.server.starts_with("nats://")
        {
            return Err(NatsError::Configuration {
                message: "NATS URLs must use tls://, wss://, or explicit loopback nats://",
            });
        }
        Ok(())
    }
}

fn nats_url_is_loopback(server: &str) -> bool {
    url::Url::parse(server).ok().is_some_and(|url| {
        url.scheme() == "nats"
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

impl fmt::Debug for NatsConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NatsConfig")
            .field("server", &"<redacted>")
            .field("client_name", &self.client_name)
            .field("connection_timeout", &self.connection_timeout)
            .field("max_reconnects", &self.max_reconnects)
            .field("max_in_flight", &self.max_in_flight)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .field("allow_plaintext_local", &self.allow_plaintext_local)
            .finish()
    }
}

/// Builder for a NATS provider.
#[derive(Clone)]
pub struct NatsProviderBuilder {
    config: NatsConfig,
    options: Option<async_nats::ConnectOptions>,
    telemetry: IntegrationTelemetry,
}

impl fmt::Debug for NatsProviderBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NatsProviderBuilder")
            .field("config", &self.config)
            .field(
                "connect_options",
                &self.options.as_ref().map(|_| "<redacted>"),
            )
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

impl NatsProviderBuilder {
    /// Creates a provider builder.
    pub fn new(config: NatsConfig) -> Self {
        Self {
            config,
            options: None,
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Replaces native connect options for credentials, certificates, callbacks, and advanced tuning.
    pub fn connect_options(mut self, options: async_nats::ConnectOptions) -> Self {
        self.options = Some(options);
        self
    }

    /// Adds shared tracing, metrics, dashboard, or custom telemetry.
    pub fn telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Connects and creates native Core NATS and JetStream clients.
    pub async fn connect(self) -> Result<NatsProvider> {
        self.config.validate()?;
        let started_at = Instant::now();
        let options = self.options.unwrap_or_else(|| {
            async_nats::ConnectOptions::new()
                .name(&self.config.client_name)
                .connection_timeout(self.config.connection_timeout)
                .max_reconnects(Some(self.config.max_reconnects))
        });
        let client = options
            .connect(self.config.server.clone())
            .await
            .map_err(nats_error);
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-nats",
                "connect",
                if client.is_ok() {
                    IntegrationStatus::Success
                } else {
                    IntegrationStatus::Failure
                },
                started_at.elapsed(),
            ))
            .await;
        let client = client?;
        Ok(NatsProvider {
            jetstream: async_nats::jetstream::new(client.clone()),
            client,
            in_flight: Arc::new(Semaphore::new(self.config.max_in_flight)),
            max_in_flight: self.config.max_in_flight as u32,
            shutting_down: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            config: self.config,
            telemetry: self.telemetry,
        })
    }

    /// Connects and registers the provider as a Nidus singleton.
    pub async fn register(self, container: &mut Container) -> Result<()> {
        container.register_singleton(self.connect().await?)?;
        Ok(())
    }
}

/// Nidus provider exposing native Core NATS and JetStream clients.
#[derive(Clone)]
pub struct NatsProvider {
    config: NatsConfig,
    client: async_nats::Client,
    jetstream: async_nats::jetstream::Context,
    in_flight: Arc<Semaphore>,
    max_in_flight: u32,
    shutting_down: Arc<AtomicBool>,
    shutdown_complete: Arc<AtomicBool>,
    telemetry: IntegrationTelemetry,
}

impl NatsProvider {
    /// Creates a NATS provider builder.
    pub fn builder(config: NatsConfig) -> NatsProviderBuilder {
        NatsProviderBuilder::new(config)
    }

    /// Returns the native Core NATS client.
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }

    /// Returns the native JetStream context.
    pub fn jetstream(&self) -> &async_nats::jetstream::Context {
        &self.jetstream
    }

    /// Publishes to Core NATS and flushes the connection buffer.
    ///
    /// Core NATS is at-most-once; use [`Self::publish_jetstream`] when durable
    /// persistence and acknowledgement are required.
    pub async fn publish(&self, subject: &str, payload: impl Into<Bytes>) -> Result<()> {
        validate_subject(subject)?;
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let result = async {
            self.client
                .publish(subject.to_owned(), payload.into())
                .await
                .map_err(nats_error)?;
            self.client.flush().await.map_err(nats_error)
        }
        .await;
        self.record("publish_core", result.is_ok(), started_at)
            .await;
        result
    }

    /// Publishes to JetStream and waits for the server persistence acknowledgement.
    pub async fn publish_jetstream(
        &self,
        subject: &str,
        payload: impl Into<Bytes>,
    ) -> Result<async_nats::jetstream::publish::PublishAck> {
        validate_subject(subject)?;
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let result = async {
            self.jetstream
                .publish(subject.to_owned(), payload.into())
                .await
                .map_err(nats_error)?
                .await
                .map_err(nats_error)
        }
        .await;
        self.record("publish_jetstream", result.is_ok(), started_at)
            .await;
        result
    }

    /// Publishes a bounded shared envelope through JetStream.
    pub async fn publish_envelope<T>(
        &self,
        subject: &str,
        envelope: &MessageEnvelope<T>,
    ) -> Result<async_nats::jetstream::publish::PublishAck>
    where
        T: Serialize,
    {
        self.publish_jetstream(subject, envelope.to_json()?).await
    }

    /// Drains subscriptions and pending publishes before closing the connection.
    pub async fn shutdown(&self) -> Result<()> {
        if self.shutdown_complete.load(Ordering::Acquire) {
            return Ok(());
        }
        self.shutting_down.store(true, Ordering::Release);
        let started_at = Instant::now();
        let result = match tokio::time::timeout(self.config.shutdown_timeout, async {
            let drained = self
                .in_flight
                .acquire_many(self.max_in_flight)
                .await
                .map_err(|_| NatsError::ShuttingDown)?;
            let result = self.client.drain().await.map_err(nats_error);
            drop(drained);
            result
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(NatsError::ShutdownTimeout),
        };
        self.record("shutdown", result.is_ok(), started_at).await;
        if result.is_ok() {
            self.shutdown_complete.store(true, Ordering::Release);
            self.in_flight.close();
        }
        result
    }

    /// Reports readiness from the native reconnecting connection state.
    #[cfg(feature = "health")]
    pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
        let started_at = Instant::now();
        let healthy = !self.shutting_down.load(Ordering::Acquire)
            && self.client.connection_state() == async_nats::connection::State::Connected;
        self.record("health", healthy, started_at).await;
        if healthy {
            nidus_http::health::HealthStatus::up()
        } else {
            nidus_http::health::HealthStatus::down("nats connection is not ready")
        }
    }

    /// Adds this provider as a readiness check on a health registry.
    #[cfg(feature = "health")]
    pub fn register_ready_check(
        self: Arc<Self>,
        registry: nidus_http::health::HealthRegistry,
        name: impl Into<String>,
    ) -> nidus_http::health::HealthRegistry {
        registry.ready_check(name, move || {
            let provider = Arc::clone(&self);
            async move { provider.health_status().await }
        })
    }

    async fn acquire_permit(&self) -> Result<SemaphorePermit<'_>> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(NatsError::ShuttingDown);
        }
        let permit = self
            .in_flight
            .acquire()
            .await
            .map_err(|_| NatsError::ShuttingDown)?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(NatsError::ShuttingDown);
        }
        Ok(permit)
    }

    async fn record(&self, operation: &'static str, success: bool, started_at: Instant) {
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-nats",
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

impl fmt::Debug for NatsProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NatsProvider")
            .field("config", &self.config)
            .field("connection_state", &self.client.connection_state())
            .field("shutting_down", &self.shutting_down.load(Ordering::Acquire))
            .field("available_permits", &self.in_flight.available_permits())
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

#[async_trait]
impl LifecycleHook for NatsProvider {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "NATS drain failed during shutdown".to_owned(),
            })
    }
}

fn validate_subject(subject: &str) -> Result<()> {
    if subject.trim().is_empty() {
        return Err(NatsError::Configuration {
            message: "NATS subject cannot be empty",
        });
    }
    Ok(())
}

fn nats_error(error: impl std::error::Error + Send + Sync + 'static) -> NatsError {
    NatsError::Nats {
        source: Box::new(error),
    }
}

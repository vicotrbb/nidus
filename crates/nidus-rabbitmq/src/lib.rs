#![deny(missing_docs)]

//! First-party RabbitMQ integration for Nidus.
//!
//! The provider exposes native Lapin connections and channels. Convenience
//! publishing enables publisher confirms and persistent messages, but consumer
//! acknowledgements, exchanges, queues, bindings, dead-letter exchanges, and
//! recovery topology remain explicit RabbitMQ capabilities.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use lapin::{
    BasicProperties, Channel, Confirmation, Connection, ConnectionProperties,
    options::{BasicPublishOptions, BasicQosOptions, ConfirmSelectOptions},
};
use nidus_core::{Container, LifecycleHook, NidusError};
use nidus_integrations::{
    IntegrationEvent, IntegrationStatus, IntegrationTelemetry, MessageEnvelope,
};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{Semaphore, SemaphorePermit};

const MAX_IN_FLIGHT_PUBLISHES: usize = 65_536;

/// Result type for RabbitMQ adapter operations.
pub type Result<T> = std::result::Result<T, RabbitMqError>;

/// RabbitMQ adapter error preserving native Lapin failures.
#[derive(Debug, Error)]
pub enum RabbitMqError {
    /// Lapin returned an AMQP or connection error.
    #[error(transparent)]
    Lapin(#[from] lapin::Error),
    /// RabbitMQ negatively acknowledged a published message.
    #[error("RabbitMQ negatively acknowledged the publication")]
    NegativeAcknowledgement,
    /// RabbitMQ returned an unroutable mandatory publication.
    #[error("RabbitMQ returned an unroutable mandatory publication")]
    Unroutable,
    /// Publisher confirms were unexpectedly not active.
    #[error("RabbitMQ publisher confirms were not active")]
    ConfirmationNotRequested,
    /// Shared envelope serialization failed.
    #[error(transparent)]
    Integration(#[from] nidus_integrations::IntegrationError),
    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),
    /// Adapter configuration was unsafe or incomplete.
    #[error("invalid RabbitMQ configuration: {message}")]
    Configuration {
        /// Redaction-safe validation message.
        message: &'static str,
    },
    /// The bounded provider was closed during an operation.
    #[error("RabbitMQ provider is shutting down")]
    ShuttingDown,
    /// Operations or native close handshakes exceeded the shutdown deadline.
    #[error("RabbitMQ shutdown exceeded its configured timeout")]
    ShutdownTimeout,
}

/// Typed RabbitMQ connection and backpressure configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct RabbitMqConfig {
    uri: String,
    consumer_prefetch: u16,
    max_in_flight_publishes: usize,
    shutdown_timeout: Duration,
    allow_plaintext_local: bool,
}

impl RabbitMqConfig {
    /// Creates TLS-first RabbitMQ configuration with publisher and consumer bounds.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            consumer_prefetch: 64,
            max_in_flight_publishes: 256,
            shutdown_timeout: Duration::from_secs(10),
            allow_plaintext_local: false,
        }
    }

    /// Sets the native channel prefetch applied to the initial channel.
    pub fn with_consumer_prefetch(mut self, value: u16) -> Self {
        self.consumer_prefetch = value;
        self
    }

    /// Sets the maximum concurrent provider-owned publications.
    pub fn with_max_in_flight_publishes(mut self, value: usize) -> Self {
        self.max_in_flight_publishes = value;
        self
    }

    /// Sets the total graceful shutdown deadline.
    pub fn with_shutdown_timeout(mut self, value: Duration) -> Self {
        self.shutdown_timeout = value;
        self
    }

    /// Explicitly permits `amqp://` for local development and tests.
    pub fn allow_plaintext_for_local_development(mut self) -> Self {
        self.allow_plaintext_local = true;
        self
    }

    /// Returns the AMQP URI.
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Validates TLS and backpressure bounds before network I/O.
    pub fn validate(&self) -> Result<()> {
        if self.uri.trim().is_empty() {
            return Err(RabbitMqError::Configuration {
                message: "RabbitMQ URI cannot be empty",
            });
        }
        if self.consumer_prefetch == 0
            || self.max_in_flight_publishes == 0
            || self.max_in_flight_publishes > MAX_IN_FLIGHT_PUBLISHES
            || self.shutdown_timeout.is_zero()
        {
            return Err(RabbitMqError::Configuration {
                message: "RabbitMQ prefetch, publish concurrency, or shutdown timeout is invalid",
            });
        }
        if self.uri.starts_with("amqp://") && !self.allow_plaintext_local {
            return Err(RabbitMqError::Configuration {
                message: "plaintext RabbitMQ requires an explicit local-development opt-in",
            });
        }
        if self.uri.starts_with("amqp://") && !rabbitmq_url_is_loopback(&self.uri) {
            return Err(RabbitMqError::Configuration {
                message: "plaintext RabbitMQ is restricted to loopback servers",
            });
        }
        if !self.uri.starts_with("amqps://") && !self.uri.starts_with("amqp://") {
            return Err(RabbitMqError::Configuration {
                message: "RabbitMQ URIs must use amqps:// or explicit loopback amqp://",
            });
        }
        Ok(())
    }
}

fn rabbitmq_url_is_loopback(uri: &str) -> bool {
    url::Url::parse(uri).ok().is_some_and(|url| {
        url.scheme() == "amqp"
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

impl fmt::Debug for RabbitMqConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RabbitMqConfig")
            .field("uri", &"<redacted>")
            .field("consumer_prefetch", &self.consumer_prefetch)
            .field("max_in_flight_publishes", &self.max_in_flight_publishes)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .field("allow_plaintext_local", &self.allow_plaintext_local)
            .finish()
    }
}

/// Builder for a RabbitMQ provider.
#[derive(Clone, Debug)]
pub struct RabbitMqProviderBuilder {
    config: RabbitMqConfig,
    telemetry: IntegrationTelemetry,
}

impl RabbitMqProviderBuilder {
    /// Creates a provider builder.
    pub fn new(config: RabbitMqConfig) -> Self {
        Self {
            config,
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Adds shared tracing, metrics, dashboard, or custom telemetry.
    pub fn telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Connects with automatic topology recovery and publisher confirms.
    pub async fn connect(self) -> Result<RabbitMqProvider> {
        self.config.validate()?;
        let started_at = Instant::now();
        let connection = Connection::connect(
            &self.config.uri,
            ConnectionProperties::default().enable_auto_recover(),
        )
        .await;
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-rabbitmq",
                "connect",
                if connection.is_ok() {
                    IntegrationStatus::Success
                } else {
                    IntegrationStatus::Failure
                },
                started_at.elapsed(),
            ))
            .await;
        let connection = Arc::new(connection?);
        let channel = connection.create_channel().await?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await?;
        channel
            .basic_qos(self.config.consumer_prefetch, BasicQosOptions::default())
            .await?;
        Ok(RabbitMqProvider {
            connection,
            channel,
            in_flight: Arc::new(Semaphore::new(self.config.max_in_flight_publishes)),
            max_in_flight: self.config.max_in_flight_publishes as u32,
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

/// Nidus provider exposing native Lapin connection and channel capabilities.
#[derive(Clone)]
pub struct RabbitMqProvider {
    config: RabbitMqConfig,
    connection: Arc<Connection>,
    channel: Channel,
    in_flight: Arc<Semaphore>,
    max_in_flight: u32,
    shutting_down: Arc<AtomicBool>,
    shutdown_complete: Arc<AtomicBool>,
    telemetry: IntegrationTelemetry,
}

impl RabbitMqProvider {
    /// Creates a RabbitMQ provider builder.
    pub fn builder(config: RabbitMqConfig) -> RabbitMqProviderBuilder {
        RabbitMqProviderBuilder::new(config)
    }

    /// Returns the native Lapin connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Returns the native publisher-confirm channel.
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    /// Creates an additional native channel for broker-specific topology or consumers.
    pub async fn create_channel(&self) -> Result<Channel> {
        Ok(self.connection.create_channel().await?)
    }

    /// Publishes a persistent mandatory message and waits for broker confirmation.
    pub async fn publish(
        &self,
        exchange: &str,
        routing_key: &str,
        payload: &[u8],
        properties: BasicProperties,
    ) -> Result<()> {
        if routing_key.trim().is_empty() {
            return Err(RabbitMqError::Configuration {
                message: "RabbitMQ routing_key cannot be empty",
            });
        }
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let outcome = async {
            let confirmation = self
                .channel
                .basic_publish(
                    exchange.into(),
                    routing_key.into(),
                    BasicPublishOptions {
                        mandatory: true,
                        ..Default::default()
                    },
                    payload,
                    properties.with_delivery_mode(2),
                )
                .await?
                .await?;
            match confirmation {
                Confirmation::Ack(None) => Ok(()),
                Confirmation::Ack(Some(_)) => Err(RabbitMqError::Unroutable),
                Confirmation::Nack(_) => Err(RabbitMqError::NegativeAcknowledgement),
                Confirmation::NotRequested => Err(RabbitMqError::ConfirmationNotRequested),
            }
        }
        .await;
        self.record("publish", outcome.is_ok(), started_at).await;
        outcome
    }

    /// Publishes a persistent JSON envelope and waits for broker confirmation.
    pub async fn publish_envelope<T>(
        &self,
        exchange: &str,
        routing_key: &str,
        envelope: &MessageEnvelope<T>,
    ) -> Result<()>
    where
        T: Serialize,
    {
        let properties = BasicProperties::default()
            .with_content_type("application/json".into())
            .with_message_id(envelope.id().into())
            .with_type(envelope.name().into());
        self.publish(exchange, routing_key, &envelope.to_json()?, properties)
            .await
    }

    /// Closes the publisher channel and connection gracefully.
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
                .map_err(|_| RabbitMqError::ShuttingDown)?;
            if self.channel.status().connected() {
                self.channel.close(200, "Nidus shutdown".into()).await?;
            }
            if self.connection.status().connected() {
                self.connection.close(200, "Nidus shutdown".into()).await?;
            }
            drop(drained);
            Ok(())
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(RabbitMqError::ShutdownTimeout),
        };
        self.record("shutdown", result.is_ok(), started_at).await;
        if result.is_ok() {
            self.shutdown_complete.store(true, Ordering::Release);
            self.in_flight.close();
        }
        result
    }

    /// Reports native connection and channel readiness.
    #[cfg(feature = "health")]
    pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
        let started_at = Instant::now();
        let healthy = !self.shutting_down.load(Ordering::Acquire)
            && self.connection.status().connected()
            && !self.connection.status().blocked()
            && self.channel.status().connected();
        self.record("health", healthy, started_at).await;
        if healthy {
            nidus_http::health::HealthStatus::up()
        } else {
            nidus_http::health::HealthStatus::down("rabbitmq connection is not ready")
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
            return Err(RabbitMqError::ShuttingDown);
        }
        let permit = self
            .in_flight
            .acquire()
            .await
            .map_err(|_| RabbitMqError::ShuttingDown)?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(RabbitMqError::ShuttingDown);
        }
        Ok(permit)
    }

    async fn record(&self, operation: &'static str, success: bool, started_at: Instant) {
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-rabbitmq",
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

impl fmt::Debug for RabbitMqProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RabbitMqProvider")
            .field("config", &self.config)
            .field("connected", &self.connection.status().connected())
            .field("channel_connected", &self.channel.status().connected())
            .field("shutting_down", &self.shutting_down.load(Ordering::Acquire))
            .field("available_permits", &self.in_flight.available_permits())
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

#[async_trait]
impl LifecycleHook for RabbitMqProvider {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "RabbitMQ shutdown failed".to_owned(),
            })
    }
}

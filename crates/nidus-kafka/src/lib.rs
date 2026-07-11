#![deny(missing_docs)]

//! First-party Apache Kafka integration for Nidus.
//!
//! The adapter preserves rust-rdkafka's native producer, consumer, admin,
//! partition, offset, rebalance, and transaction APIs. Convenience publishing
//! waits for Kafka delivery reports and defaults to idempotent production, but
//! Nidus does not claim end-to-end exactly-once processing.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use nidus_core::{Container, LifecycleHook, NidusError};
use nidus_integrations::{
    IntegrationEvent, IntegrationStatus, IntegrationTelemetry, MessageEnvelope,
};
use rdkafka::{
    ClientConfig, Message,
    admin::AdminClient,
    client::DefaultClientContext,
    consumer::StreamConsumer,
    error::KafkaError as NativeKafkaError,
    message::{Header, OwnedHeaders},
    producer::{FutureProducer, FutureRecord, Producer},
};
use serde::Serialize;
use thiserror::Error;

/// Result type for Kafka adapter operations.
pub type Result<T> = std::result::Result<T, KafkaError>;

/// Kafka adapter error preserving native rust-rdkafka errors.
#[derive(Debug, Error)]
pub enum KafkaError {
    /// rust-rdkafka returned an error.
    #[error(transparent)]
    Kafka(#[from] NativeKafkaError),
    /// A delivery report failed after the message entered the producer queue.
    #[error("Kafka delivery failed: {source}")]
    Delivery {
        /// Native delivery failure.
        #[source]
        source: NativeKafkaError,
    },
    /// A blocking librdkafka operation could not be joined.
    #[error("Kafka blocking task failed")]
    TaskJoin,
    /// Shared envelope serialization failed.
    #[error(transparent)]
    Integration(#[from] nidus_integrations::IntegrationError),
    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),
    /// Adapter configuration was unsafe or incomplete.
    #[error("invalid Kafka configuration: {message}")]
    Configuration {
        /// Redaction-safe validation message.
        message: &'static str,
    },
    /// The provider stopped accepting adapter-owned publications.
    #[error("Kafka provider is shutting down")]
    ShuttingDown,
}

/// Typed Kafka client configuration with secure production defaults.
#[derive(Clone, Eq, PartialEq)]
pub struct KafkaConfig {
    bootstrap_servers: String,
    client_id: String,
    properties: BTreeMap<String, String>,
    enqueue_timeout: Duration,
    shutdown_timeout: Duration,
    allow_plaintext_local: bool,
}

impl KafkaConfig {
    /// Creates Kafka configuration using TLS, idempotent production, and bounded queues.
    pub fn new(bootstrap_servers: impl Into<String>, client_id: impl Into<String>) -> Self {
        Self {
            bootstrap_servers: bootstrap_servers.into(),
            client_id: client_id.into(),
            properties: BTreeMap::from([
                ("security.protocol".to_owned(), "SSL".to_owned()),
                ("enable.idempotence".to_owned(), "true".to_owned()),
                ("acks".to_owned(), "all".to_owned()),
                ("message.timeout.ms".to_owned(), "30000".to_owned()),
                (
                    "queue.buffering.max.messages".to_owned(),
                    "100000".to_owned(),
                ),
            ]),
            enqueue_timeout: Duration::from_secs(5),
            shutdown_timeout: Duration::from_secs(10),
            allow_plaintext_local: false,
        }
    }

    /// Adds or replaces a native librdkafka property.
    pub fn property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Explicitly permits a plaintext broker for local development and tests.
    pub fn allow_plaintext_for_local_development(mut self) -> Self {
        self.properties
            .insert("security.protocol".to_owned(), "PLAINTEXT".to_owned());
        self.allow_plaintext_local = true;
        self
    }

    /// Sets how long a producer call may wait for local queue capacity.
    pub fn with_enqueue_timeout(mut self, value: Duration) -> Self {
        self.enqueue_timeout = value;
        self
    }

    /// Sets the graceful producer flush timeout.
    pub fn with_shutdown_timeout(mut self, value: Duration) -> Self {
        self.shutdown_timeout = value;
        self
    }

    /// Returns the bootstrap server list.
    pub fn bootstrap_servers(&self) -> &str {
        &self.bootstrap_servers
    }

    /// Returns the stable client id.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Returns configured native property names and values.
    pub fn properties(&self) -> &BTreeMap<String, String> {
        &self.properties
    }

    /// Validates safe, bounded client settings before native client creation.
    pub fn validate(&self) -> Result<()> {
        if self.bootstrap_servers.trim().is_empty() {
            return Err(KafkaError::Configuration {
                message: "bootstrap_servers cannot be empty",
            });
        }
        if self.client_id.trim().is_empty() {
            return Err(KafkaError::Configuration {
                message: "client_id cannot be empty",
            });
        }
        if self.enqueue_timeout.is_zero() || self.shutdown_timeout.is_zero() {
            return Err(KafkaError::Configuration {
                message: "Kafka timeouts must be greater than zero",
            });
        }
        let protocol = self
            .properties
            .get("security.protocol")
            .map(String::as_str)
            .unwrap_or("PLAINTEXT");
        let plaintext = protocol.eq_ignore_ascii_case("PLAINTEXT")
            || protocol.eq_ignore_ascii_case("SASL_PLAINTEXT");
        if plaintext && !self.allow_plaintext_local {
            return Err(KafkaError::Configuration {
                message: "plaintext Kafka requires an explicit local-development opt-in",
            });
        }
        if plaintext && !kafka_bootstrap_is_loopback(&self.bootstrap_servers) {
            return Err(KafkaError::Configuration {
                message: "plaintext Kafka is restricted to loopback brokers",
            });
        }
        Ok(())
    }

    fn native_config(&self) -> Result<ClientConfig> {
        self.validate()?;
        let mut config = ClientConfig::new();
        config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set("client.id", &self.client_id);
        for (key, value) in &self.properties {
            config.set(key, value);
        }
        Ok(config)
    }
}

fn kafka_bootstrap_is_loopback(bootstrap_servers: &str) -> bool {
    bootstrap_servers
        .split(',')
        .all(|server| kafka_broker_is_loopback(server.trim()))
}

fn kafka_broker_is_loopback(server: &str) -> bool {
    if server.is_empty() || server.contains('@') {
        return false;
    }
    let Some((host, port)) = server.rsplit_once(':') else {
        return false;
    };
    if port.parse::<u16>().is_err() {
        return false;
    }
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

impl fmt::Debug for KafkaConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KafkaConfig")
            .field("bootstrap_servers", &"<redacted>")
            .field("client_id", &self.client_id)
            .field(
                "property_names",
                &self.properties.keys().collect::<Vec<_>>(),
            )
            .field("enqueue_timeout", &self.enqueue_timeout)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .field("allow_plaintext_local", &self.allow_plaintext_local)
            .finish()
    }
}

/// Successful Kafka delivery coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KafkaDelivery {
    partition: i32,
    offset: i64,
}

impl KafkaDelivery {
    /// Returns the destination partition.
    pub const fn partition(&self) -> i32 {
        self.partition
    }

    /// Returns the committed log offset from the delivery report.
    pub const fn offset(&self) -> i64 {
        self.offset
    }
}

/// Builder for a Kafka provider.
#[derive(Clone, Debug)]
pub struct KafkaProviderBuilder {
    config: KafkaConfig,
    telemetry: IntegrationTelemetry,
}

impl KafkaProviderBuilder {
    /// Creates a provider builder.
    pub fn new(config: KafkaConfig) -> Self {
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

    /// Creates native Kafka clients without blocking on broker connectivity.
    pub fn build(self) -> Result<KafkaProvider> {
        let native = self.config.native_config()?;
        let producer = native.create::<FutureProducer>()?;
        let admin = native.create::<AdminClient<DefaultClientContext>>()?;
        Ok(KafkaProvider {
            config: self.config,
            producer,
            admin: Arc::new(admin),
            shutting_down: Arc::new(AtomicBool::new(false)),
            telemetry: self.telemetry,
        })
    }

    /// Builds and registers the provider as a Nidus singleton.
    pub fn register(self, container: &mut Container) -> Result<()> {
        container.register_singleton(self.build()?)?;
        Ok(())
    }
}

/// Nidus provider exposing native rust-rdkafka clients.
#[derive(Clone)]
pub struct KafkaProvider {
    config: KafkaConfig,
    producer: FutureProducer,
    admin: Arc<AdminClient<DefaultClientContext>>,
    shutting_down: Arc<AtomicBool>,
    telemetry: IntegrationTelemetry,
}

impl KafkaProvider {
    /// Creates a Kafka provider builder.
    pub fn builder(config: KafkaConfig) -> KafkaProviderBuilder {
        KafkaProviderBuilder::new(config)
    }

    /// Returns the native asynchronous producer.
    pub fn producer(&self) -> &FutureProducer {
        &self.producer
    }

    /// Returns the native Kafka admin client.
    pub fn admin(&self) -> &AdminClient<DefaultClientContext> {
        &self.admin
    }

    /// Creates a native manual-commit stream consumer for a group.
    ///
    /// Auto commit and auto offset storage are disabled so applications can
    /// store or commit offsets only after successful processing.
    pub fn consumer(&self, group_id: &str) -> Result<StreamConsumer> {
        if group_id.trim().is_empty() {
            return Err(KafkaError::Configuration {
                message: "Kafka consumer group_id cannot be empty",
            });
        }
        let mut config = self.config.native_config()?;
        config
            .set("group.id", group_id)
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("isolation.level", "read_committed");
        Ok(config.create()?)
    }

    /// Publishes bytes and waits for the native Kafka delivery report.
    pub async fn publish(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        payload: &[u8],
    ) -> Result<KafkaDelivery> {
        self.publish_with_headers(topic, key, payload, None).await
    }

    /// Publishes a shared envelope with correlation and trace headers.
    pub async fn publish_envelope<T>(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        envelope: &MessageEnvelope<T>,
    ) -> Result<KafkaDelivery>
    where
        T: Serialize,
    {
        let payload = envelope.to_json()?;
        let mut headers = OwnedHeaders::new()
            .insert(Header {
                key: "nidus-message-id",
                value: Some(envelope.id()),
            })
            .insert(Header {
                key: "nidus-message-name",
                value: Some(envelope.name()),
            });
        if let Some(value) = envelope.metadata().correlation_id_value() {
            headers = headers.insert(Header {
                key: "nidus-correlation-id",
                value: Some(value),
            });
        }
        if let Some(value) = envelope.metadata().traceparent_value() {
            headers = headers.insert(Header {
                key: "traceparent",
                value: Some(value),
            });
        }
        self.publish_with_headers(topic, key, &payload, Some(headers))
            .await
    }

    /// Flushes queued delivery reports without blocking a Tokio worker thread.
    pub async fn shutdown(&self) -> Result<()> {
        self.shutting_down.store(true, Ordering::Release);
        let producer = self.producer.clone();
        let timeout = self.config.shutdown_timeout;
        tokio::task::spawn_blocking(move || producer.flush(timeout))
            .await
            .map_err(|_| KafkaError::TaskJoin)??;
        Ok(())
    }

    /// Checks broker metadata without blocking a Tokio worker thread.
    #[cfg(feature = "health")]
    pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
        if self.shutting_down.load(Ordering::Acquire) {
            return nidus_http::health::HealthStatus::down("kafka provider is shutting down");
        }
        let started_at = Instant::now();
        let producer = self.producer.clone();
        let result = tokio::task::spawn_blocking(move || {
            producer
                .client()
                .fetch_metadata(None, Duration::from_secs(2))
        })
        .await;
        let healthy = matches!(result, Ok(Ok(_)));
        self.record("health", healthy, started_at).await;
        if healthy {
            nidus_http::health::HealthStatus::up()
        } else {
            nidus_http::health::HealthStatus::down("kafka metadata check failed")
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

    async fn publish_with_headers(
        &self,
        topic: &str,
        key: Option<&[u8]>,
        payload: &[u8],
        headers: Option<OwnedHeaders>,
    ) -> Result<KafkaDelivery> {
        if topic.trim().is_empty() {
            return Err(KafkaError::Configuration {
                message: "Kafka topic cannot be empty",
            });
        }
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(KafkaError::ShuttingDown);
        }
        let started_at = Instant::now();
        let mut record = FutureRecord::to(topic).payload(payload);
        if let Some(key) = key {
            record = record.key(key);
        }
        if let Some(headers) = headers {
            record = record.headers(headers);
        }
        let result = self
            .producer
            .send(record, self.config.enqueue_timeout)
            .await;
        self.record("publish", result.is_ok(), started_at).await;
        match result {
            Ok(delivery) => Ok(KafkaDelivery {
                partition: delivery.partition,
                offset: delivery.offset,
            }),
            Err((source, message)) => {
                let _ = message.payload();
                Err(KafkaError::Delivery { source })
            }
        }
    }

    async fn record(&self, operation: &'static str, success: bool, started_at: Instant) {
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-kafka",
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

impl fmt::Debug for KafkaProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KafkaProvider")
            .field("config", &self.config)
            .field("producer", &"FutureProducer")
            .field("admin", &"AdminClient")
            .field("shutting_down", &self.shutting_down.load(Ordering::Acquire))
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

#[async_trait]
impl LifecycleHook for KafkaProvider {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "Kafka producer flush failed during shutdown".to_owned(),
            })
    }
}

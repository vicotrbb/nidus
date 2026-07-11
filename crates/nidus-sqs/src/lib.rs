#![deny(missing_docs)]

//! First-party AWS SQS integration for Nidus.
//!
//! The adapter exposes the official AWS SDK client and preserves SQS standard,
//! FIFO, visibility timeout, long polling, message attributes, and DLQ APIs.
//! Standard queues are at-least-once; handlers must be idempotent and delete a
//! receipt only after successful processing.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use aws_sdk_sqs::types::Message;
#[cfg(feature = "health")]
use aws_sdk_sqs::types::QueueAttributeName;
use nidus_core::{Container, LifecycleHook, NidusError};
use nidus_integrations::{
    IntegrationEvent, IntegrationStatus, IntegrationTelemetry, MessageEnvelope,
};
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{Semaphore, SemaphorePermit};

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_IN_FLIGHT: usize = 65_536;

/// Result type for SQS adapter operations.
pub type Result<T> = std::result::Result<T, SqsError>;

/// SQS adapter error preserving the official SDK source error.
#[derive(Debug, Error)]
pub enum SqsError {
    /// The AWS SDK returned an SQS error.
    #[error("AWS SQS operation failed: {source}")]
    Sdk {
        /// Top-level AWS SQS source error.
        #[source]
        source: Box<aws_sdk_sqs::Error>,
    },
    /// Shared envelope serialization failed.
    #[error(transparent)]
    Integration(#[from] nidus_integrations::IntegrationError),
    /// Nidus provider registration failed.
    #[error(transparent)]
    Nidus(#[from] NidusError),
    /// Adapter configuration was unsafe or incomplete.
    #[error("invalid SQS configuration: {message}")]
    Configuration {
        /// Redaction-safe validation message.
        message: &'static str,
    },
    /// The message exceeded the SQS one-mebibyte limit.
    #[error("SQS message is {actual} bytes, exceeding the {maximum}-byte limit")]
    MessageTooLarge {
        /// Actual encoded size.
        actual: usize,
        /// SQS maximum size.
        maximum: usize,
    },
    /// An envelope was not valid UTF-8 JSON.
    #[error("SQS envelope encoding was not UTF-8")]
    InvalidUtf8,
    /// The bounded provider was closed during an operation.
    #[error("SQS provider is shutting down")]
    ShuttingDown,
    /// Admitted AWS requests did not drain before the shutdown deadline.
    #[error("SQS shutdown exceeded its configured timeout")]
    ShutdownTimeout,
}

/// Typed SQS long-polling, visibility, and concurrency configuration.
#[derive(Clone, Eq, PartialEq)]
pub struct SqsConfig {
    queue_url: String,
    wait_time_seconds: i32,
    visibility_timeout_seconds: i32,
    max_messages: i32,
    max_in_flight: usize,
    shutdown_timeout: Duration,
    allow_http_local: bool,
}

impl SqsConfig {
    /// Creates secure long-polling SQS configuration.
    pub fn new(queue_url: impl Into<String>) -> Self {
        Self {
            queue_url: queue_url.into(),
            wait_time_seconds: 20,
            visibility_timeout_seconds: 60,
            max_messages: 10,
            max_in_flight: 64,
            shutdown_timeout: Duration::from_secs(30),
            allow_http_local: false,
        }
    }

    /// Sets the long-poll wait time in the SQS range `0..=20` seconds.
    pub fn with_wait_time_seconds(mut self, value: i32) -> Self {
        self.wait_time_seconds = value;
        self
    }

    /// Sets the per-receive visibility timeout in `0..=43200` seconds.
    pub fn with_visibility_timeout_seconds(mut self, value: i32) -> Self {
        self.visibility_timeout_seconds = value;
        self
    }

    /// Sets the receive batch size in the SQS range `1..=10`.
    pub fn with_max_messages(mut self, value: i32) -> Self {
        self.max_messages = value;
        self
    }

    /// Sets the maximum concurrent provider-owned AWS requests.
    pub fn with_max_in_flight(mut self, value: usize) -> Self {
        self.max_in_flight = value;
        self
    }

    /// Sets how long graceful shutdown waits for admitted AWS requests.
    pub fn with_shutdown_timeout(mut self, value: Duration) -> Self {
        self.shutdown_timeout = value;
        self
    }

    /// Explicitly permits an HTTP queue endpoint for LocalStack-style tests.
    pub fn allow_http_for_local_development(mut self) -> Self {
        self.allow_http_local = true;
        self
    }

    /// Returns the configured queue URL.
    pub fn queue_url(&self) -> &str {
        &self.queue_url
    }

    /// Validates SQS service limits and TLS before client creation.
    pub fn validate(&self) -> Result<()> {
        if self.queue_url.trim().is_empty() {
            return Err(SqsError::Configuration {
                message: "SQS queue_url cannot be empty",
            });
        }
        if !(0..=20).contains(&self.wait_time_seconds)
            || !(0..=43_200).contains(&self.visibility_timeout_seconds)
            || !(1..=10).contains(&self.max_messages)
            || self.max_in_flight == 0
            || self.max_in_flight > MAX_IN_FLIGHT
            || self.shutdown_timeout.is_zero()
        {
            return Err(SqsError::Configuration {
                message: "SQS polling, visibility, batch, or concurrency bounds are invalid",
            });
        }
        if self.queue_url.starts_with("http://") && !self.allow_http_local {
            return Err(SqsError::Configuration {
                message: "HTTP SQS endpoints require an explicit local-development opt-in",
            });
        }
        if self.queue_url.starts_with("http://") && !sqs_url_is_loopback(&self.queue_url) {
            return Err(SqsError::Configuration {
                message: "HTTP SQS endpoints are restricted to loopback",
            });
        }
        if !self.queue_url.starts_with("https://") && !self.queue_url.starts_with("http://") {
            return Err(SqsError::Configuration {
                message: "SQS queue URLs must use HTTPS or explicit loopback HTTP",
            });
        }
        Ok(())
    }
}

fn sqs_url_is_loopback(url: &str) -> bool {
    url::Url::parse(url).ok().is_some_and(|url| {
        url.scheme() == "http"
            && match url.host() {
                Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(url::Host::Ipv4(host)) => host.is_loopback(),
                Some(url::Host::Ipv6(host)) => host.is_loopback(),
                None => false,
            }
    })
}

impl fmt::Debug for SqsConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqsConfig")
            .field("queue_url", &"<redacted>")
            .field("wait_time_seconds", &self.wait_time_seconds)
            .field(
                "visibility_timeout_seconds",
                &self.visibility_timeout_seconds,
            )
            .field("max_messages", &self.max_messages)
            .field("max_in_flight", &self.max_in_flight)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .field("allow_http_local", &self.allow_http_local)
            .finish()
    }
}

/// Successful SQS send identifiers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqsSendReceipt {
    message_id: Option<String>,
    sequence_number: Option<String>,
}

impl SqsSendReceipt {
    /// Returns the AWS-assigned message id, when provided.
    pub fn message_id(&self) -> Option<&str> {
        self.message_id.as_deref()
    }

    /// Returns the FIFO sequence number, when provided.
    pub fn sequence_number(&self) -> Option<&str> {
        self.sequence_number.as_deref()
    }
}

/// Builder for an SQS provider using either an injected or environment client.
#[derive(Clone)]
pub struct SqsProviderBuilder {
    config: SqsConfig,
    client: Option<aws_sdk_sqs::Client>,
    telemetry: IntegrationTelemetry,
}

impl SqsProviderBuilder {
    /// Creates a builder using the standard AWS credential and region chain.
    pub fn new(config: SqsConfig) -> Self {
        Self {
            config,
            client: None,
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Creates a builder with an existing official AWS SDK client.
    pub fn from_client(config: SqsConfig, client: aws_sdk_sqs::Client) -> Self {
        Self {
            config,
            client: Some(client),
            telemetry: IntegrationTelemetry::new(),
        }
    }

    /// Adds shared tracing, metrics, dashboard, or custom telemetry.
    pub fn telemetry(mut self, telemetry: IntegrationTelemetry) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Loads AWS configuration when needed and creates the provider.
    pub async fn connect(self) -> Result<SqsProvider> {
        self.config.validate()?;
        let client = match self.client {
            Some(client) => client,
            None => {
                let sdk_config =
                    aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                aws_sdk_sqs::Client::new(&sdk_config)
            }
        };
        Ok(SqsProvider {
            in_flight: Arc::new(Semaphore::new(self.config.max_in_flight)),
            max_in_flight: self.config.max_in_flight as u32,
            shutting_down: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            config: self.config,
            client,
            telemetry: self.telemetry,
        })
    }

    /// Creates and registers the provider as a Nidus singleton.
    pub async fn register(self, container: &mut Container) -> Result<()> {
        container.register_singleton(self.connect().await?)?;
        Ok(())
    }
}

impl fmt::Debug for SqsProviderBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqsProviderBuilder")
            .field("config", &self.config)
            .field("client", &self.client.as_ref().map(|_| "injected"))
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

/// Nidus provider exposing the official AWS SQS SDK client.
#[derive(Clone)]
pub struct SqsProvider {
    config: SqsConfig,
    client: aws_sdk_sqs::Client,
    in_flight: Arc<Semaphore>,
    max_in_flight: u32,
    shutting_down: Arc<AtomicBool>,
    shutdown_complete: Arc<AtomicBool>,
    telemetry: IntegrationTelemetry,
}

impl SqsProvider {
    /// Creates an SQS provider builder using the AWS environment chain.
    pub fn builder(config: SqsConfig) -> SqsProviderBuilder {
        SqsProviderBuilder::new(config)
    }

    /// Creates an SQS provider builder with an existing native client.
    pub fn builder_with_client(
        config: SqsConfig,
        client: aws_sdk_sqs::Client,
    ) -> SqsProviderBuilder {
        SqsProviderBuilder::from_client(config, client)
    }

    /// Returns the native official AWS SQS client.
    pub fn client(&self) -> &aws_sdk_sqs::Client {
        &self.client
    }

    /// Sends a standard-queue UTF-8 message.
    pub async fn send(&self, body: impl Into<String>) -> Result<SqsSendReceipt> {
        self.send_inner(body.into(), None, None).await
    }

    /// Sends a FIFO message with explicit group and deduplication ids.
    pub async fn send_fifo(
        &self,
        body: impl Into<String>,
        message_group_id: impl Into<String>,
        deduplication_id: impl Into<String>,
    ) -> Result<SqsSendReceipt> {
        self.send_inner(
            body.into(),
            Some(message_group_id.into()),
            Some(deduplication_id.into()),
        )
        .await
    }

    /// Sends a bounded shared envelope to a standard queue.
    pub async fn send_envelope<T>(&self, envelope: &MessageEnvelope<T>) -> Result<SqsSendReceipt>
    where
        T: Serialize,
    {
        let body = String::from_utf8(envelope.to_json()?).map_err(|_| SqsError::InvalidUtf8)?;
        self.send(body).await
    }

    /// Long-polls and returns native AWS SDK message values.
    ///
    /// Messages remain in flight until [`Self::delete`] succeeds or their
    /// visibility timeout expires. Duplicate delivery remains possible.
    pub async fn receive(&self) -> Result<Vec<Message>> {
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let output = self
            .client
            .receive_message()
            .queue_url(&self.config.queue_url)
            .wait_time_seconds(self.config.wait_time_seconds)
            .visibility_timeout(self.config.visibility_timeout_seconds)
            .max_number_of_messages(self.config.max_messages)
            .send()
            .await
            .map_err(sdk_error);
        self.record("receive", output.is_ok(), started_at).await;
        Ok(output?.messages().to_vec())
    }

    /// Deletes a successfully processed message by receipt handle.
    pub async fn delete(&self, receipt_handle: &str) -> Result<()> {
        if receipt_handle.is_empty() {
            return Err(SqsError::Configuration {
                message: "SQS receipt_handle cannot be empty",
            });
        }
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let result = self
            .client
            .delete_message()
            .queue_url(&self.config.queue_url)
            .receipt_handle(receipt_handle)
            .send()
            .await
            .map_err(sdk_error);
        self.record("delete", result.is_ok(), started_at).await;
        result?;
        Ok(())
    }

    /// Extends or terminates a message visibility lease.
    pub async fn change_visibility(
        &self,
        receipt_handle: &str,
        visibility_timeout_seconds: i32,
    ) -> Result<()> {
        if receipt_handle.is_empty() || !(0..=43_200).contains(&visibility_timeout_seconds) {
            return Err(SqsError::Configuration {
                message: "SQS receipt handle or visibility timeout is invalid",
            });
        }
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let result = self
            .client
            .change_message_visibility()
            .queue_url(&self.config.queue_url)
            .receipt_handle(receipt_handle)
            .visibility_timeout(visibility_timeout_seconds)
            .send()
            .await
            .map_err(sdk_error);
        self.record("change_visibility", result.is_ok(), started_at)
            .await;
        result?;
        Ok(())
    }

    /// Checks queue access with `GetQueueAttributes`.
    #[cfg(feature = "health")]
    pub async fn health_status(&self) -> nidus_http::health::HealthStatus {
        let Ok(_permit) = self.acquire_permit().await else {
            return nidus_http::health::HealthStatus::down("sqs provider is shutting down");
        };
        let started_at = Instant::now();
        let result = self
            .client
            .get_queue_attributes()
            .queue_url(&self.config.queue_url)
            .attribute_names(QueueAttributeName::QueueArn)
            .send()
            .await;
        self.record("health", result.is_ok(), started_at).await;
        if result.is_ok() {
            nidus_http::health::HealthStatus::up()
        } else {
            nidus_http::health::HealthStatus::down("sqs queue check failed")
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

    /// Stops new adapter-owned work and waits for admitted AWS requests.
    ///
    /// The official SDK client has no transport close handshake; injected
    /// native client clones remain under application ownership.
    pub async fn shutdown(&self) -> Result<()> {
        if self.shutdown_complete.load(Ordering::Acquire) {
            return Ok(());
        }
        self.shutting_down.store(true, Ordering::Release);
        let started_at = Instant::now();
        let result = match tokio::time::timeout(
            self.config.shutdown_timeout,
            self.in_flight.acquire_many(self.max_in_flight),
        )
        .await
        {
            Ok(Ok(drained)) => {
                self.shutdown_complete.store(true, Ordering::Release);
                self.in_flight.close();
                drop(drained);
                Ok(())
            }
            Ok(Err(_)) => Err(SqsError::ShuttingDown),
            Err(_) => Err(SqsError::ShutdownTimeout),
        };
        self.record("shutdown", result.is_ok(), started_at).await;
        result
    }

    async fn send_inner(
        &self,
        body: String,
        message_group_id: Option<String>,
        deduplication_id: Option<String>,
    ) -> Result<SqsSendReceipt> {
        if body.len() > MAX_MESSAGE_BYTES {
            return Err(SqsError::MessageTooLarge {
                actual: body.len(),
                maximum: MAX_MESSAGE_BYTES,
            });
        }
        let _permit = self.acquire_permit().await?;
        let started_at = Instant::now();
        let output = self
            .client
            .send_message()
            .queue_url(&self.config.queue_url)
            .message_body(body)
            .set_message_group_id(message_group_id)
            .set_message_deduplication_id(deduplication_id)
            .send()
            .await
            .map_err(sdk_error);
        self.record("send", output.is_ok(), started_at).await;
        let output = output?;
        Ok(SqsSendReceipt {
            message_id: output.message_id().map(str::to_owned),
            sequence_number: output.sequence_number().map(str::to_owned),
        })
    }

    async fn record(&self, operation: &'static str, success: bool, started_at: Instant) {
        self.telemetry
            .record(&IntegrationEvent::new(
                "nidus-sqs",
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

    async fn acquire_permit(&self) -> Result<SemaphorePermit<'_>> {
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(SqsError::ShuttingDown);
        }
        let permit = self
            .in_flight
            .acquire()
            .await
            .map_err(|_| SqsError::ShuttingDown)?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(SqsError::ShuttingDown);
        }
        Ok(permit)
    }
}

impl fmt::Debug for SqsProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqsProvider")
            .field("config", &self.config)
            .field("client", &"aws_sdk_sqs::Client")
            .field("shutting_down", &self.shutting_down.load(Ordering::Acquire))
            .field("available_permits", &self.in_flight.available_permits())
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

#[async_trait]
impl LifecycleHook for SqsProvider {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "SQS requests failed to drain during shutdown".to_owned(),
            })
    }
}

fn sdk_error(error: impl Into<aws_sdk_sqs::Error>) -> SqsError {
    SqsError::Sdk {
        source: Box::new(error.into()),
    }
}

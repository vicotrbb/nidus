#![deny(missing_docs)]

//! Shared primitives for first-party Nidus integration crates.
//!
//! This crate deliberately does not define a universal queue or database API.
//! Broker and data adapters expose their native clients. The types here cover
//! only stable cross-cutting concerns: envelopes, correlation, redaction-safe
//! diagnostics, and best-effort lifecycle telemetry.

use std::{collections::BTreeMap, fmt, panic::AssertUnwindSafe, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

/// Default maximum serialized envelope size: one mebibyte.
pub const DEFAULT_MAX_ENVELOPE_BYTES: usize = 1024 * 1024;

const MAX_NAME_BYTES: usize = 255;
const MAX_CORRELATION_BYTES: usize = 255;
const MAX_HEADER_COUNT: usize = 64;
const MAX_HEADER_NAME_BYTES: usize = 128;
const MAX_HEADER_VALUE_BYTES: usize = 4096;

/// Result type for shared integration operations.
pub type Result<T> = std::result::Result<T, IntegrationError>;

/// Validation or serialization error from shared integration primitives.
#[derive(Debug, Error)]
pub enum IntegrationError {
    /// A stable envelope name was empty or too large.
    #[error("message name must contain 1..={MAX_NAME_BYTES} bytes")]
    InvalidMessageName,
    /// A correlation or causation identifier exceeded the safe bound.
    #[error("correlation identifiers must contain 1..={MAX_CORRELATION_BYTES} bytes")]
    InvalidCorrelationId,
    /// A W3C traceparent value was malformed.
    #[error("traceparent is not a valid W3C trace context value")]
    InvalidTraceparent,
    /// An envelope header name was malformed or exceeded its bound.
    #[error("header names must contain 1..={MAX_HEADER_NAME_BYTES} visible ASCII bytes")]
    InvalidHeaderName,
    /// An envelope header value exceeded its bound or contained a control byte.
    #[error("header values must contain at most {MAX_HEADER_VALUE_BYTES} visible bytes")]
    InvalidHeaderValue,
    /// The configured header count bound was exceeded.
    #[error("an envelope may contain at most {MAX_HEADER_COUNT} headers")]
    TooManyHeaders,
    /// The serialized envelope exceeded the configured transport bound.
    #[error("serialized envelope is {actual} bytes, exceeding the {maximum}-byte limit")]
    EnvelopeTooLarge {
        /// Actual serialized or inbound size.
        actual: usize,
        /// Configured maximum size.
        maximum: usize,
    },
    /// JSON serialization or deserialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Metadata carried with a message without imposing broker semantics.
#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnvelopeMetadata {
    correlation_id: Option<String>,
    causation_id: Option<String>,
    traceparent: Option<String>,
    headers: BTreeMap<String, String>,
}

impl EnvelopeMetadata {
    /// Creates empty envelope metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a bounded correlation identifier.
    pub fn correlation_id(mut self, correlation_id: impl Into<String>) -> Result<Self> {
        self.correlation_id = Some(validate_correlation_id(correlation_id.into())?);
        Ok(self)
    }

    /// Sets a bounded causation identifier.
    pub fn causation_id(mut self, causation_id: impl Into<String>) -> Result<Self> {
        self.causation_id = Some(validate_correlation_id(causation_id.into())?);
        Ok(self)
    }

    /// Sets validated W3C trace context for downstream propagation.
    pub fn traceparent(mut self, traceparent: impl Into<String>) -> Result<Self> {
        let traceparent = traceparent.into();
        if !valid_traceparent(&traceparent) {
            return Err(IntegrationError::InvalidTraceparent);
        }
        self.traceparent = Some(traceparent);
        Ok(self)
    }

    /// Adds a bounded transport-neutral header.
    ///
    /// Header values remain available to application code but are never shown
    /// by this type's `Debug` implementation.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let value = value.into();
        validate_header_name(&name)?;
        validate_header_value(&value)?;
        if !self.headers.contains_key(&name) && self.headers.len() == MAX_HEADER_COUNT {
            return Err(IntegrationError::TooManyHeaders);
        }
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Returns the correlation identifier, when present.
    pub fn correlation_id_value(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    /// Returns the causation identifier, when present.
    pub fn causation_id_value(&self) -> Option<&str> {
        self.causation_id.as_deref()
    }

    /// Returns the propagated W3C traceparent, when present.
    pub fn traceparent_value(&self) -> Option<&str> {
        self.traceparent.as_deref()
    }

    /// Returns application-visible envelope headers.
    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }
}

impl fmt::Debug for EnvelopeMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvelopeMetadata")
            .field("correlation_id", &self.correlation_id)
            .field("causation_id", &self.causation_id)
            .field("traceparent", &self.traceparent)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Versioned transport-neutral message envelope.
///
/// The payload is intentionally omitted from `Debug` output to prevent
/// accidental secret or PII disclosure. Delivery guarantees remain the
/// responsibility of each native broker adapter.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessageEnvelope<T> {
    id: String,
    name: String,
    schema_version: u32,
    occurred_at_ms: i64,
    metadata: EnvelopeMetadata,
    payload: T,
}

impl<T> MessageEnvelope<T> {
    /// Creates an envelope with a generated UUID and current UTC timestamp.
    pub fn new(name: impl Into<String>, payload: T) -> Result<Self> {
        Self::from_parts(
            uuid::Uuid::new_v4().to_string(),
            name,
            1,
            time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000,
            EnvelopeMetadata::new(),
            payload,
        )
    }

    /// Creates an envelope from explicit, validated parts.
    pub fn from_parts(
        id: impl Into<String>,
        name: impl Into<String>,
        schema_version: u32,
        occurred_at_ms: i64,
        metadata: EnvelopeMetadata,
        payload: T,
    ) -> Result<Self> {
        let id = validate_correlation_id(id.into())?;
        let name = name.into();
        if name.is_empty() || name.len() > MAX_NAME_BYTES {
            return Err(IntegrationError::InvalidMessageName);
        }
        Ok(Self {
            id,
            name,
            schema_version,
            occurred_at_ms,
            metadata,
            payload,
        })
    }

    /// Replaces the envelope metadata.
    pub fn with_metadata(mut self, metadata: EnvelopeMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Returns the stable message identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the stable message name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the application schema version.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the UTC occurrence timestamp in Unix milliseconds.
    pub const fn occurred_at_ms(&self) -> i64 {
        self.occurred_at_ms
    }

    /// Returns the envelope metadata.
    pub fn metadata(&self) -> &EnvelopeMetadata {
        &self.metadata
    }

    /// Returns the typed payload.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the envelope and returns the typed payload.
    pub fn into_payload(self) -> T {
        self.payload
    }
}

impl<T> MessageEnvelope<T>
where
    T: Serialize,
{
    /// Serializes the envelope as bounded JSON.
    pub fn to_json(&self) -> Result<Vec<u8>> {
        self.to_json_with_limit(DEFAULT_MAX_ENVELOPE_BYTES)
    }

    /// Serializes the envelope as JSON with an explicit byte limit.
    pub fn to_json_with_limit(&self, maximum: usize) -> Result<Vec<u8>> {
        let encoded = serde_json::to_vec(self)?;
        if encoded.len() > maximum {
            return Err(IntegrationError::EnvelopeTooLarge {
                actual: encoded.len(),
                maximum,
            });
        }
        Ok(encoded)
    }
}

impl<T> MessageEnvelope<T>
where
    T: DeserializeOwned,
{
    /// Deserializes a bounded JSON envelope.
    pub fn from_json(encoded: &[u8]) -> Result<Self> {
        Self::from_json_with_limit(encoded, DEFAULT_MAX_ENVELOPE_BYTES)
    }

    /// Deserializes JSON after enforcing an explicit inbound byte limit.
    pub fn from_json_with_limit(encoded: &[u8], maximum: usize) -> Result<Self> {
        if encoded.len() > maximum {
            return Err(IntegrationError::EnvelopeTooLarge {
                actual: encoded.len(),
                maximum,
            });
        }
        let envelope: Self = serde_json::from_slice(encoded)?;
        Self::from_parts(
            envelope.id,
            envelope.name,
            envelope.schema_version,
            envelope.occurred_at_ms,
            envelope.metadata,
            envelope.payload,
        )
    }
}

impl<T> fmt::Debug for MessageEnvelope<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MessageEnvelope")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("schema_version", &self.schema_version)
            .field("occurred_at_ms", &self.occurred_at_ms)
            .field("metadata", &self.metadata)
            .field("payload", &"<redacted>")
            .finish()
    }
}

/// Outcome of a first-party adapter operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationStatus {
    /// The operation completed successfully.
    Success,
    /// The operation failed.
    Failure,
    /// The operation was cancelled during shutdown.
    Cancelled,
}

/// Redaction-safe lifecycle event emitted by an integration adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrationEvent {
    adapter: &'static str,
    operation: &'static str,
    status: IntegrationStatus,
    duration: Duration,
    correlation_id: Option<String>,
}

impl IntegrationEvent {
    /// Creates a lifecycle event using stable adapter and operation names.
    pub fn new(
        adapter: &'static str,
        operation: &'static str,
        status: IntegrationStatus,
        duration: Duration,
    ) -> Self {
        Self {
            adapter,
            operation,
            status,
            duration,
            correlation_id: None,
        }
    }

    /// Adds a bounded correlation identifier when it is valid.
    pub fn correlation_id(mut self, correlation_id: impl Into<String>) -> Result<Self> {
        self.correlation_id = Some(validate_correlation_id(correlation_id.into())?);
        Ok(self)
    }

    /// Returns the stable adapter name.
    pub const fn adapter(&self) -> &'static str {
        self.adapter
    }

    /// Returns the stable operation name.
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns the operation outcome.
    pub const fn status(&self) -> IntegrationStatus {
        self.status
    }

    /// Returns the operation duration.
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the correlation identifier, when present.
    pub fn correlation_id_value(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
}

/// Best-effort observer for integration lifecycle operations.
///
/// Observers must not include message payloads, raw URLs, credentials, queue
/// names containing tenant data, or other high-cardinality values in telemetry.
#[async_trait]
pub trait IntegrationObserver: Send + Sync + 'static {
    /// Records one redaction-safe integration event.
    async fn record(&self, event: &IntegrationEvent);
}

/// Composite best-effort telemetry used by first-party adapters.
#[derive(Clone, Default)]
pub struct IntegrationTelemetry {
    observers: Arc<Vec<Arc<dyn IntegrationObserver>>>,
}

impl IntegrationTelemetry {
    /// Creates telemetry with no observers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a custom integration observer.
    pub fn with_observer<O>(self, observer: O) -> Self
    where
        O: IntegrationObserver,
    {
        let mut observers = self.observers.as_ref().clone();
        observers.push(Arc::new(observer));
        Self {
            observers: Arc::new(observers),
        }
    }

    /// Adds a redaction-safe `tracing` observer.
    pub fn tracing(self) -> Self {
        self.with_observer(TracingIntegrationObserver)
    }

    /// Adds Nidus Prometheus adapter metrics.
    #[cfg(feature = "observability")]
    pub fn observability(
        self,
        observer: nidus_observability::ObservabilityAdapterObserver,
    ) -> Self {
        self.with_observer(ObservabilityIntegrationObserver(observer))
    }

    /// Adds Nidus Dashboard adapter timeline capture.
    #[cfg(feature = "dashboard")]
    pub fn dashboard(
        self,
        collector: nidus_dashboard::DashboardCollector<
            nidus_dashboard::storage::DashboardStorageHandle,
        >,
    ) -> Self {
        self.with_observer(DashboardIntegrationObserver(collector))
    }

    /// Returns whether no telemetry observers are configured.
    pub fn is_empty(&self) -> bool {
        self.observers.is_empty()
    }

    /// Records an event to all observers in registration order.
    pub async fn record(&self, event: &IntegrationEvent) {
        for observer in self.observers.iter() {
            if AssertUnwindSafe(observer.record(event))
                .catch_unwind()
                .await
                .is_err()
            {
                tracing::warn!(
                    adapter = event.adapter(),
                    operation = event.operation(),
                    "integration telemetry observer panicked and was isolated"
                );
            }
        }
    }
}

impl fmt::Debug for IntegrationTelemetry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IntegrationTelemetry")
            .field("observer_count", &self.observers.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
struct TracingIntegrationObserver;

#[async_trait]
impl IntegrationObserver for TracingIntegrationObserver {
    async fn record(&self, event: &IntegrationEvent) {
        tracing::info!(
            adapter = event.adapter(),
            operation = event.operation(),
            status = ?event.status(),
            duration_ms = event.duration().as_millis(),
            correlation_id = event.correlation_id_value(),
            "integration operation completed"
        );
    }
}

#[cfg(feature = "observability")]
#[derive(Clone, Debug)]
struct ObservabilityIntegrationObserver(nidus_observability::ObservabilityAdapterObserver);

#[cfg(feature = "observability")]
#[async_trait]
impl IntegrationObserver for ObservabilityIntegrationObserver {
    async fn record(&self, event: &IntegrationEvent) {
        let status = match event.status() {
            IntegrationStatus::Success => nidus_observability::OperationStatus::Success,
            IntegrationStatus::Failure | IntegrationStatus::Cancelled => {
                nidus_observability::OperationStatus::Failure
            }
        };
        self.0
            .record(event.adapter(), event.operation(), status, event.duration());
    }
}

#[cfg(feature = "dashboard")]
#[derive(Clone, Debug)]
struct DashboardIntegrationObserver(
    nidus_dashboard::DashboardCollector<nidus_dashboard::storage::DashboardStorageHandle>,
);

#[cfg(feature = "dashboard")]
#[async_trait]
impl IntegrationObserver for DashboardIntegrationObserver {
    async fn record(&self, event: &IntegrationEvent) {
        if let Err(error) = self
            .0
            .record_adapter(
                format!("{}.{}", event.adapter(), event.operation()),
                event.correlation_id_value(),
                event.status() == IntegrationStatus::Success,
                event.duration().as_millis().min(u128::from(u64::MAX)) as u64,
            )
            .await
        {
            tracing::warn!(error = %error, "dashboard adapter telemetry was dropped");
        }
    }
}

fn validate_correlation_id(value: String) -> Result<String> {
    if value.is_empty()
        || value.len() > MAX_CORRELATION_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(IntegrationError::InvalidCorrelationId);
    }
    Ok(value)
}

fn validate_header_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > MAX_HEADER_NAME_BYTES
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(IntegrationError::InvalidHeaderName);
    }
    Ok(())
}

fn validate_header_value(value: &str) -> Result<()> {
    if value.len() > MAX_HEADER_VALUE_BYTES || value.chars().any(char::is_control) {
        return Err(IntegrationError::InvalidHeaderValue);
    }
    Ok(())
}

fn valid_traceparent(value: &str) -> bool {
    let mut parts = value.split('-');
    let (Some(version), Some(trace_id), Some(parent_id), Some(flags)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if parts.next().is_some() || version == "ff" || version.len() != 2 {
        return false;
    }
    let valid_lower_hex = |part: &str, length: usize| {
        part.len() == length
            && part
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    valid_lower_hex(version, 2)
        && valid_lower_hex(trace_id, 32)
        && trace_id.bytes().any(|byte| byte != b'0')
        && valid_lower_hex(parent_id, 16)
        && parent_id.bytes().any(|byte| byte != b'0')
        && valid_lower_hex(flags, 2)
}

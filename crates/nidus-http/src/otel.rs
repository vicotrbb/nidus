//! Backend-optional OpenTelemetry helpers.

use std::{collections::BTreeMap, future::Future};

use http::{HeaderMap, HeaderValue};
use tracing::Instrument;

/// OpenTelemetry configuration for service resources and OTLP export settings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OtelConfig {
    service_name: String,
    otlp_endpoint: Option<String>,
    resource_attributes: BTreeMap<String, String>,
}

impl OtelConfig {
    /// Creates OpenTelemetry config for a service.
    pub fn new(service_name: impl Into<String>) -> Self {
        let service_name = service_name.into();
        let mut resource_attributes = BTreeMap::new();
        resource_attributes.insert("service.name".to_owned(), service_name.clone());
        Self {
            service_name,
            otlp_endpoint: None,
            resource_attributes,
        }
    }

    /// Sets the service version resource attribute.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.resource_attributes
            .insert("service.version".to_owned(), version.into());
        self
    }

    /// Sets the deployment environment resource attribute.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        self.resource_attributes
            .insert("deployment.environment".to_owned(), environment.into());
        self
    }

    /// Sets the OTLP endpoint for exporters that use this config.
    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Adds or replaces a resource attribute.
    pub fn resource_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.resource_attributes.insert(key.into(), value.into());
        self
    }

    /// Returns the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Returns the configured OTLP endpoint.
    pub fn otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }

    /// Returns resource attributes for OpenTelemetry exporters.
    pub fn resource_attributes(&self) -> &BTreeMap<String, String> {
        &self.resource_attributes
    }
}

/// W3C trace context extracted from or injected into `traceparent`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceContext {
    trace_id: String,
    span_id: String,
    sampled: bool,
}

impl TraceContext {
    /// Creates trace context from validated parts.
    pub fn new(trace_id: impl Into<String>, span_id: impl Into<String>, sampled: bool) -> Self {
        Self {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            sampled,
        }
    }

    /// Parses a W3C `traceparent` header.
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('-');
        let version = parts.next()?;
        let trace_id = parts.next()?;
        let span_id = parts.next()?;
        let flags = parts.next()?;
        if parts.next().is_some()
            || version.len() != 2
            || trace_id.len() != 32
            || span_id.len() != 16
            || flags.len() != 2
            || !is_lower_hex(version)
            || !is_lower_hex(trace_id)
            || !is_lower_hex(span_id)
            || !is_lower_hex(flags)
            || trace_id.chars().all(|character| character == '0')
            || span_id.chars().all(|character| character == '0')
        {
            return None;
        }
        let flags = u8::from_str_radix(flags, 16).ok()?;
        Some(Self::new(trace_id, span_id, flags & 1 == 1))
    }

    /// Returns the trace id.
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Returns the span id.
    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    /// Returns whether the sampled flag is set.
    pub const fn sampled(&self) -> bool {
        self.sampled
    }

    /// Formats this context as a W3C `traceparent` value.
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id,
            self.span_id,
            if self.sampled { 1 } else { 0 }
        )
    }
}

/// Extracts W3C trace context from HTTP headers.
pub fn extract_trace_context(headers: &HeaderMap) -> Option<TraceContext> {
    headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .and_then(TraceContext::parse)
}

/// Injects W3C trace context into HTTP headers.
pub fn inject_trace_context(headers: &mut HeaderMap, context: &TraceContext) {
    if let Ok(value) = HeaderValue::from_str(&context.to_traceparent()) {
        headers.insert("traceparent", value);
    }
}

/// Runs a future inside an observed tracing span.
pub async fn with_observed_span<Fut, T>(operation: &'static str, future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    future
        .instrument(tracing::info_span!("operation", otel.name = operation))
        .await
}

/// Records exception fields on the current span without requiring a concrete OTel backend.
pub fn record_exception(error: &(dyn std::error::Error + 'static)) {
    tracing::Span::current().record("exception.message", tracing::field::display(error));
}

/// Shutdown hook trait for OpenTelemetry exporters.
pub trait OtelShutdown: Send + Sync + 'static {
    /// Flushes and shuts down an exporter or tracer provider.
    fn shutdown(&self);
}

/// Runs an optional OpenTelemetry shutdown hook.
pub fn shutdown_otel(shutdown: Option<&dyn OtelShutdown>) {
    if let Some(shutdown) = shutdown {
        shutdown.shutdown();
    }
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

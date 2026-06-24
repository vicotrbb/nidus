//! Structured logging helpers built on `tracing` and `tracing-subscriber`.

use std::{borrow::Cow, collections::BTreeSet};

use http::Request;
use tower_http::trace::MakeSpan;
use tracing::{Level, Span};
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{self, MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::context::header_to_string;

/// Structured logging output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoggingFormat {
    /// JSON logs for production log pipelines.
    Json,
    /// Pretty logs for local development.
    Pretty,
}

/// Typed configuration for Nidus logging helpers.
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    service_name: String,
    version: Option<String>,
    environment: Option<String>,
    format: LoggingFormat,
    level_filter: String,
    redacted_headers: BTreeSet<String>,
}

impl LoggingConfig {
    /// Creates production JSON logging config for a service.
    pub fn production(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            version: None,
            environment: None,
            format: LoggingFormat::Json,
            level_filter: "info".to_owned(),
            redacted_headers: BTreeSet::new(),
        }
    }

    /// Creates development pretty logging config for a service.
    pub fn development(service_name: impl Into<String>) -> Self {
        Self::production(service_name).with_format(LoggingFormat::Pretty)
    }

    /// Returns the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Sets the service version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Sets the deployment environment.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Sets the logging format.
    pub fn with_format(mut self, format: LoggingFormat) -> Self {
        self.format = format;
        self
    }

    /// Sets the tracing level filter directive.
    pub fn level_filter(mut self, level_filter: impl Into<String>) -> Self {
        self.level_filter = level_filter.into();
        self
    }

    /// Marks a header as redacted for application log code.
    pub fn redact_header(mut self, header: impl AsRef<str>) -> Self {
        self.redacted_headers
            .insert(header.as_ref().to_ascii_lowercase());
        self
    }

    /// Returns whether the config redacts a header name.
    pub fn redacts_header(&self, header: impl AsRef<str>) -> bool {
        self.redacted_headers
            .contains(&header.as_ref().to_ascii_lowercase())
    }

    /// Returns the configured output format.
    pub const fn output_format(&self) -> LoggingFormat {
        self.format
    }

    /// Returns the configured output format.
    pub const fn format(&self) -> LoggingFormat {
        self.format
    }

    /// Creates a root service span carrying stable deployment attributes.
    pub fn service_span(&self) -> Span {
        tracing::info_span!(
            "service",
            service.name = %self.service_name,
            service.version = %self.version.as_deref().unwrap_or(""),
            deployment.environment = %self.environment.as_deref().unwrap_or("")
        )
    }

    /// Installs this config as the process-global tracing subscriber.
    pub fn init(&self) -> Result<(), tracing_subscriber::util::TryInitError> {
        match self.format {
            LoggingFormat::Json => self.subscriber_with_writer(std::io::stderr).try_init(),
            LoggingFormat::Pretty => self
                .pretty_subscriber_with_writer(std::io::stderr)
                .try_init(),
        }
    }

    /// Builds a JSON subscriber using a caller-provided writer.
    pub fn subscriber_with_writer<W>(
        &self,
        writer: W,
    ) -> impl tracing::Subscriber + Send + Sync + 'static
    where
        W: for<'writer> MakeWriter<'writer> + Clone + Send + Sync + 'static,
    {
        let filter =
            EnvFilter::try_new(&self.level_filter).unwrap_or_else(|_| EnvFilter::new("info"));
        let layer = fmt::layer()
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .with_filter(filter);
        Registry::default().with(layer)
    }

    /// Builds a pretty subscriber using a caller-provided writer.
    pub fn pretty_subscriber_with_writer<W>(
        &self,
        writer: W,
    ) -> impl tracing::Subscriber + Send + Sync + 'static
    where
        W: for<'writer> MakeWriter<'writer> + Clone + Send + Sync + 'static,
    {
        let filter =
            EnvFilter::try_new(&self.level_filter).unwrap_or_else(|_| EnvFilter::new("info"));
        let layer = fmt::layer()
            .pretty()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .with_filter(filter);
        Registry::default().with(layer)
    }
}

/// Span maker that records service, request, route, and trace context fields.
#[derive(Clone, Debug)]
pub struct StructuredMakeSpan {
    config: LoggingConfig,
    route: Option<Cow<'static, str>>,
}

impl StructuredMakeSpan {
    /// Creates a structured HTTP span maker.
    pub fn new(config: LoggingConfig) -> Self {
        Self {
            config,
            route: None,
        }
    }

    /// Sets the stable route pattern for spans made by this value.
    pub fn route(mut self, route: impl Into<Cow<'static, str>>) -> Self {
        self.route = Some(route.into());
        self
    }
}

impl<B> MakeSpan<B> for StructuredMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let request_id = header_to_string(request.headers(), "x-request-id").unwrap_or_default();
        let trace_id = header_to_string(request.headers(), "traceparent")
            .and_then(|value| value.split('-').nth(1).map(str::to_owned))
            .unwrap_or_default();
        let route = self
            .route
            .as_deref()
            .map(str::to_owned)
            .or_else(|| {
                request
                    .extensions()
                    .get::<axum::extract::MatchedPath>()
                    .map(|path| path.as_str().to_owned())
            })
            .unwrap_or_else(|| "<unknown>".to_owned());

        tracing::span!(
            Level::INFO,
            "http.request",
            service.name = %self.config.service_name,
            service.version = %self.config.version.as_deref().unwrap_or(""),
            deployment.environment = %self.config.environment.as_deref().unwrap_or(""),
            request.id = %request_id,
            trace.id = %trace_id,
            http.method = %request.method(),
            http.route = %route,
            http.target = %request.uri(),
        )
    }
}

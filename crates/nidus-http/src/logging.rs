//! Structured logging helpers built on `tracing` and `tracing-subscriber`.

use std::borrow::Cow;

use http::Request;
use tower_http::trace::MakeSpan;
use tracing::{Level, Span};
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{self, MakeWriter},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::context::{header_to_str, parse_traceparent};

/// Structured logging output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoggingFormat {
    /// JSON logs for production log pipelines.
    Json,
    /// Pretty logs for local development.
    Pretty,
}

/// Typed configuration for Nidus logging helpers.
///
/// `LoggingConfig` builds `tracing-subscriber` subscribers and structured
/// service/request spans. It does not install HTTP middleware by itself; pair it
/// with `tower_http::trace::TraceLayer` and [`StructuredMakeSpan`] for request
/// spans.
///
/// ```no_run
/// use nidus_http::logging::{LoggingConfig, StructuredMakeSpan};
/// use tower_http::trace::TraceLayer;
///
/// let logging = LoggingConfig::production("users-api")
///     .version("1.2.3")
///     .environment("production")
///     .level_filter("info,tower_http=debug")
///     .redact_header("authorization");
///
/// logging.init()?;
/// let trace_layer = TraceLayer::new_for_http()
///     .make_span_with(StructuredMakeSpan::new(logging));
/// # Ok::<(), tracing_subscriber::util::TryInitError>(())
/// ```
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    service_name: String,
    version: Option<String>,
    environment: Option<String>,
    format: LoggingFormat,
    level_filter: String,
    // Kept sorted after ASCII normalization so request-time lookups can use
    // binary search without allocating a lowercase `String`.
    redacted_headers: Vec<String>,
}

impl LoggingConfig {
    /// Creates production JSON logging config for a service.
    ///
    /// Defaults to JSON output, `info` filtering, and no redacted headers.
    pub fn production(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            version: None,
            environment: None,
            format: LoggingFormat::Json,
            level_filter: "info".to_owned(),
            redacted_headers: Vec::new(),
        }
    }

    /// Creates development pretty logging config for a service.
    ///
    /// This keeps the same service metadata defaults as production but uses
    /// pretty text formatting.
    pub fn development(service_name: impl Into<String>) -> Self {
        Self::production(service_name).with_format(LoggingFormat::Pretty)
    }

    /// Returns the service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Sets the service version.
    ///
    /// The version is included in [`Self::service_span`] and
    /// [`StructuredMakeSpan`] fields.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Sets the deployment environment.
    ///
    /// The environment is included in [`Self::service_span`] and
    /// [`StructuredMakeSpan`] fields.
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
    ///
    /// This stores redaction policy for callers via [`Self::redacts_header`].
    /// The built-in [`StructuredMakeSpan`] does not log arbitrary request
    /// headers, so there is no automatic header scrubber to install.
    pub fn redact_header(mut self, header: impl AsRef<str>) -> Self {
        let header = header.as_ref().to_ascii_lowercase();
        if let Err(index) = self.redacted_headers.binary_search(&header) {
            self.redacted_headers.insert(index, header);
        }
        self
    }

    /// Returns whether the config redacts a header name.
    pub fn redacts_header(&self, header: impl AsRef<str>) -> bool {
        let header = header.as_ref();
        self.redacted_headers
            .binary_search_by(|configured| {
                configured
                    .bytes()
                    .cmp(header.bytes().map(|byte| byte.to_ascii_lowercase()))
            })
            .is_ok()
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
    ///
    /// Like other `tracing-subscriber` global installs, this usually succeeds
    /// once per process. Tests often prefer [`Self::subscriber_with_writer`] to
    /// avoid global state.
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
///
/// The span includes `service.name`, `service.version`,
/// `deployment.environment`, `request.id`, `trace.id`, `http.method`,
/// `http.route`, and `http.target`. Request ID and trace ID are read from
/// `x-request-id` and `traceparent` headers respectively; use the request ID
/// middleware before tracing when you need every request span to have an ID.
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
    ///
    /// When unset, the span maker falls back to Axum's
    /// [`axum::extract::MatchedPath`] extension and then `"<unknown>"`.
    pub fn route(mut self, route: impl Into<Cow<'static, str>>) -> Self {
        self.route = Some(route.into());
        self
    }
}

impl<B> MakeSpan<B> for StructuredMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let request_id = header_to_str(request.headers(), "x-request-id").unwrap_or_default();
        let trace_id = header_to_str(request.headers(), "traceparent")
            .and_then(parse_traceparent)
            .map(|context| context.trace_id)
            .unwrap_or_default();
        let route = self
            .route
            .as_deref()
            .or_else(|| {
                request
                    .extensions()
                    .get::<axum::extract::MatchedPath>()
                    .map(axum::extract::MatchedPath::as_str)
            })
            .unwrap_or("<unknown>");

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

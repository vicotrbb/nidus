#![deny(missing_docs)]

//! Production OpenTelemetry tracing for Nidus.
//!
//! The pipeline uses the official SDK, a bounded batch processor, the
//! `tracing` bridge, W3C Trace Context and baggage propagation, and either an
//! OTLP gRPC or OTLP/HTTP protobuf exporter. It does not install a process-
//! global tracing subscriber, so tests and applications retain lifecycle
//! control.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use http::{HeaderMap, HeaderName, HeaderValue};
use nidus_core::{Container, LifecycleHook, NidusError};
use opentelemetry::{
    Context, KeyValue,
    propagation::{Extractor, Injector, TextMapCompositePropagator, TextMapPropagator},
    trace::TracerProvider as _,
};
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig, WithTonicConfig};
use opentelemetry_sdk::{
    Resource,
    propagation::{BaggagePropagator, TraceContextPropagator},
    trace::{
        BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracer, SdkTracerProvider, SpanExporter,
    },
};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::registry::LookupSpan;

const MAX_HEADER_COUNT: usize = 32;
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_QUEUE_SIZE: usize = 65_536;

/// Result type for OpenTelemetry pipeline operations.
pub type Result<T> = std::result::Result<T, OpenTelemetryError>;

/// Error returned while validating, starting, flushing, or stopping telemetry.
#[derive(Debug, thiserror::Error)]
pub enum OpenTelemetryError {
    /// Configuration is unsafe or internally inconsistent.
    #[error("invalid OpenTelemetry configuration: {0}")]
    Configuration(String),
    /// The OTLP exporter could not be built.
    #[error("failed to build OTLP exporter: {0}")]
    Exporter(#[from] opentelemetry_otlp::ExporterBuildError),
    /// The SDK failed to flush or shut down.
    #[error("OpenTelemetry SDK operation failed: {0}")]
    Sdk(#[from] opentelemetry_sdk::error::OTelSdkError),
    /// A blocking SDK lifecycle task failed to join.
    #[error("OpenTelemetry lifecycle task failed: {0}")]
    TaskJoin(String),
}

/// OTLP trace transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OtlpProtocol {
    /// OTLP over gRPC using Tonic.
    Grpc,
    /// OTLP over HTTP with protobuf payloads.
    HttpProtobuf,
}

/// Redaction-safe, bounded OTLP pipeline configuration.
#[derive(Clone)]
pub struct OpenTelemetryConfig {
    service_name: String,
    service_version: Option<String>,
    environment: Option<String>,
    endpoint: String,
    protocol: OtlpProtocol,
    headers: BTreeMap<String, String>,
    timeout: Duration,
    queue_size: usize,
    batch_size: usize,
    scheduled_delay: Duration,
    sample_ratio: f64,
    allow_insecure_local_endpoint: bool,
}

impl OpenTelemetryConfig {
    /// Creates a secure OTLP/gRPC configuration.
    pub fn grpc(service_name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self::new(service_name, endpoint, OtlpProtocol::Grpc)
    }

    /// Creates a secure OTLP/HTTP protobuf configuration.
    pub fn http_protobuf(service_name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self::new(service_name, endpoint, OtlpProtocol::HttpProtobuf)
    }

    /// Loads endpoint and protocol from standard OTLP environment variables.
    ///
    /// The endpoint must be explicit so production never silently falls back to
    /// the upstream plaintext localhost default.
    pub fn from_env(service_name: impl Into<String>) -> Result<Self> {
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
            .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
            .map_err(|_| {
                OpenTelemetryError::Configuration(
                    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT or OTEL_EXPORTER_OTLP_ENDPOINT is required"
                        .to_owned(),
                )
            })?;
        let protocol = std::env::var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL")
            .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL"))
            .unwrap_or_else(|_| "grpc".to_owned());
        match protocol.as_str() {
            "grpc" => Ok(Self::grpc(service_name, endpoint)),
            "http/protobuf" => Ok(Self::http_protobuf(service_name, endpoint)),
            _ => Err(OpenTelemetryError::Configuration(
                "OTLP protocol must be grpc or http/protobuf".to_owned(),
            )),
        }
    }

    fn new(
        service_name: impl Into<String>,
        endpoint: impl Into<String>,
        protocol: OtlpProtocol,
    ) -> Self {
        Self {
            service_name: service_name.into(),
            service_version: None,
            environment: None,
            endpoint: endpoint.into(),
            protocol,
            headers: BTreeMap::new(),
            timeout: Duration::from_secs(10),
            queue_size: 2_048,
            batch_size: 512,
            scheduled_delay: Duration::from_secs(5),
            sample_ratio: 1.0,
            allow_insecure_local_endpoint: false,
        }
    }

    /// Sets the service version resource attribute.
    pub fn with_service_version(mut self, version: impl Into<String>) -> Self {
        self.service_version = Some(version.into());
        self
    }

    /// Sets the deployment environment resource attribute.
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Adds a bounded exporter header, such as an authorization token.
    ///
    /// Values are intentionally omitted from `Debug` output.
    pub fn with_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into().to_ascii_lowercase();
        let value = value.into();
        HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            OpenTelemetryError::Configuration("invalid OTLP header name".to_owned())
        })?;
        HeaderValue::from_str(&value).map_err(|_| {
            OpenTelemetryError::Configuration("invalid OTLP header value".to_owned())
        })?;
        if value.len() > MAX_HEADER_BYTES {
            return Err(OpenTelemetryError::Configuration(
                "OTLP header value exceeds 8192 bytes".to_owned(),
            ));
        }
        if !self.headers.contains_key(&name) && self.headers.len() == MAX_HEADER_COUNT {
            return Err(OpenTelemetryError::Configuration(
                "OTLP header count exceeds 32".to_owned(),
            ));
        }
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Sets exporter request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(120) {
            return Err(OpenTelemetryError::Configuration(
                "OTLP timeout must be between 1 millisecond and 120 seconds".to_owned(),
            ));
        }
        self.timeout = timeout;
        Ok(self)
    }

    /// Sets bounded batch processor parameters.
    pub fn with_batching(
        mut self,
        queue_size: usize,
        batch_size: usize,
        scheduled_delay: Duration,
    ) -> Result<Self> {
        if queue_size == 0
            || queue_size > MAX_QUEUE_SIZE
            || batch_size == 0
            || batch_size > queue_size
            || scheduled_delay.is_zero()
            || scheduled_delay > Duration::from_secs(60)
        {
            return Err(OpenTelemetryError::Configuration(
                "OTLP batching requires 1 <= batch <= queue <= 65536 and delay <= 60s".to_owned(),
            ));
        }
        self.queue_size = queue_size;
        self.batch_size = batch_size;
        self.scheduled_delay = scheduled_delay;
        Ok(self)
    }

    /// Sets parent-based trace ID ratio sampling.
    pub fn with_sample_ratio(mut self, ratio: f64) -> Result<Self> {
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return Err(OpenTelemetryError::Configuration(
                "trace sample ratio must be between 0 and 1".to_owned(),
            ));
        }
        self.sample_ratio = ratio;
        Ok(self)
    }

    /// Allows plaintext only for an explicit loopback collector endpoint.
    pub fn allow_insecure_local_endpoint(mut self) -> Result<Self> {
        if !is_loopback_endpoint(&self.endpoint) {
            return Err(OpenTelemetryError::Configuration(
                "insecure OTLP endpoints are restricted to loopback".to_owned(),
            ));
        }
        self.allow_insecure_local_endpoint = true;
        Ok(self)
    }

    /// Returns the stable service name.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Returns the selected exporter protocol.
    pub const fn protocol(&self) -> OtlpProtocol {
        self.protocol
    }

    fn validate(&self) -> Result<()> {
        if self.service_name.is_empty() || self.service_name.len() > 255 {
            return Err(OpenTelemetryError::Configuration(
                "service name must contain 1..=255 bytes".to_owned(),
            ));
        }
        let uri: http::Uri = self.endpoint.parse().map_err(|_| {
            OpenTelemetryError::Configuration("OTLP endpoint is not a valid URI".to_owned())
        })?;
        let scheme = uri.scheme_str().unwrap_or_default();
        if scheme != "https"
            && !(scheme == "http"
                && self.allow_insecure_local_endpoint
                && is_loopback_endpoint(&self.endpoint))
        {
            return Err(OpenTelemetryError::Configuration(
                "OTLP endpoint must use HTTPS unless loopback plaintext is explicitly enabled"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for OpenTelemetryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenTelemetryConfig")
            .field("service_name", &self.service_name)
            .field("service_version", &self.service_version)
            .field("environment", &self.environment)
            .field("endpoint", &"<redacted>")
            .field("protocol", &self.protocol)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field("timeout", &self.timeout)
            .field("queue_size", &self.queue_size)
            .field("batch_size", &self.batch_size)
            .field("scheduled_delay", &self.scheduled_delay)
            .field("sample_ratio", &self.sample_ratio)
            .field(
                "allow_insecure_local_endpoint",
                &self.allow_insecure_local_endpoint,
            )
            .finish()
    }
}

/// Owned SDK provider, tracing bridge factory, and W3C propagator.
#[derive(Clone)]
pub struct OpenTelemetryPipeline {
    provider: SdkTracerProvider,
    tracer: SdkTracer,
    propagator: Arc<TextMapCompositePropagator>,
    shutdown: Arc<AtomicBool>,
    shutdown_timeout: Duration,
}

impl OpenTelemetryPipeline {
    /// Builds a real OTLP exporter and bounded SDK batch pipeline.
    pub fn init(config: OpenTelemetryConfig) -> Result<Self> {
        config.validate()?;
        let exporter = match config.protocol {
            OtlpProtocol::Grpc => {
                let metadata = tonic_metadata(&config.headers)?;
                opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_endpoint(config.endpoint.clone())
                    .with_timeout(config.timeout)
                    .with_metadata(metadata)
                    .build()?
            }
            OtlpProtocol::HttpProtobuf => opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(config.endpoint.clone())
                .with_timeout(config.timeout)
                .with_headers(
                    config
                        .headers
                        .clone()
                        .into_iter()
                        .collect::<HashMap<_, _>>(),
                )
                .build()?,
        };
        Self::from_exporter(config, exporter)
    }

    /// Builds the same SDK pipeline around a custom exporter.
    ///
    /// This is useful for first-party tests and private exporters while keeping
    /// batching, resources, sampling, propagation, and shutdown identical.
    pub fn from_exporter<E>(config: OpenTelemetryConfig, exporter: E) -> Result<Self>
    where
        E: SpanExporter + 'static,
    {
        config.validate()?;
        let mut attributes = Vec::new();
        if let Some(version) = &config.service_version {
            attributes.push(KeyValue::new("service.version", version.clone()));
        }
        if let Some(environment) = &config.environment {
            attributes.push(KeyValue::new(
                "deployment.environment.name",
                environment.clone(),
            ));
        }
        let resource = Resource::builder()
            .with_service_name(config.service_name.clone())
            .with_attributes(attributes)
            .build();
        let batch = BatchSpanProcessor::builder(exporter)
            .with_batch_config(
                BatchConfigBuilder::default()
                    .with_max_queue_size(config.queue_size)
                    .with_max_export_batch_size(config.batch_size)
                    .with_scheduled_delay(config.scheduled_delay)
                    .build(),
            )
            .build();
        let provider = SdkTracerProvider::builder()
            .with_span_processor(batch)
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                config.sample_ratio,
            ))))
            .with_resource(resource)
            .build();
        let tracer = provider.tracer(config.service_name);
        Ok(Self {
            provider,
            tracer,
            propagator: Arc::new(composite_propagator()),
            shutdown: Arc::new(AtomicBool::new(false)),
            shutdown_timeout: config.timeout,
        })
    }

    /// Returns a `tracing_subscriber` layer backed by this SDK provider.
    pub fn tracing_layer<S>(&self) -> tracing_opentelemetry::OpenTelemetryLayer<S, SdkTracer>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span>,
    {
        tracing_opentelemetry::layer().with_tracer(self.tracer.clone())
    }

    /// Returns the native SDK tracer provider.
    pub const fn provider(&self) -> &SdkTracerProvider {
        &self.provider
    }

    /// Returns the native SDK tracer.
    pub const fn tracer(&self) -> &SdkTracer {
        &self.tracer
    }

    /// Registers this pipeline as a typed singleton dependency.
    pub fn register(&self, container: &mut Container) -> nidus_core::Result<()> {
        container.register_singleton(self.clone())
    }

    /// Injects the current `tracing` span into HTTP headers using W3C formats.
    pub fn inject_current_context(&self, headers: &mut HeaderMap) {
        let context = tracing::Span::current().context();
        self.propagator
            .inject_context(&context, &mut HeaderInjector(headers));
    }

    /// Extracts W3C trace context and baggage from HTTP headers.
    pub fn extract_context(&self, headers: &HeaderMap) -> Context {
        self.propagator.extract(&HeaderExtractor(headers))
    }

    /// Sets a tracing span's remote parent from HTTP headers.
    pub fn set_parent_from_headers(
        &self,
        span: &tracing::Span,
        headers: &HeaderMap,
    ) -> std::result::Result<(), tracing_opentelemetry::SetParentError> {
        span.set_parent(self.extract_context(headers))
    }

    /// Installs W3C trace context and baggage as the process-global propagator.
    ///
    /// Most Nidus applications should prefer the scoped header methods. Global
    /// installation is explicit because OpenTelemetry has no restoration guard.
    pub fn install_global_propagator(&self) {
        opentelemetry::global::set_text_map_propagator(composite_propagator());
    }

    /// Returns whether the provider remains available for export.
    pub fn is_ready(&self) -> bool {
        !self.shutdown.load(Ordering::Acquire)
    }

    /// Adds SDK lifecycle state as a Nidus readiness check.
    #[cfg(feature = "health")]
    pub fn register_ready_check(
        self: Arc<Self>,
        registry: nidus_http::health::HealthRegistry,
        name: impl Into<String>,
    ) -> nidus_http::health::HealthRegistry {
        registry.ready_check(name, move || {
            let pipeline = Arc::clone(&self);
            async move {
                if pipeline.is_ready() {
                    nidus_http::health::HealthStatus::up()
                } else {
                    nidus_http::health::HealthStatus::down("OpenTelemetry pipeline has shut down")
                }
            }
        })
    }

    /// Records redaction-safe SDK readiness in the Nidus dashboard timeline.
    #[cfg(feature = "dashboard")]
    pub async fn record_dashboard_status(
        &self,
        collector: &nidus_dashboard::DashboardCollector<
            nidus_dashboard::storage::DashboardStorageHandle,
        >,
    ) -> nidus_dashboard::Result<()> {
        collector
            .record_adapter("nidus-opentelemetry.readiness", None, self.is_ready(), 0)
            .await
    }

    /// Flushes all currently queued spans off the async runtime worker.
    pub async fn force_flush(&self) -> Result<()> {
        if !self.is_ready() {
            return Err(OpenTelemetryError::Sdk(
                opentelemetry_sdk::error::OTelSdkError::AlreadyShutdown,
            ));
        }
        let provider = self.provider.clone();
        tokio::task::spawn_blocking(move || provider.force_flush())
            .await
            .map_err(|error| OpenTelemetryError::TaskJoin(error.to_string()))??;
        Ok(())
    }

    /// Flushes and stops the SDK batch processor exactly once.
    pub async fn shutdown(&self) -> Result<()> {
        if self
            .shutdown
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }
        let provider = self.provider.clone();
        let timeout = self.shutdown_timeout;
        tokio::task::spawn_blocking(move || provider.shutdown_with_timeout(timeout))
            .await
            .map_err(|error| OpenTelemetryError::TaskJoin(error.to_string()))??;
        Ok(())
    }
}

impl fmt::Debug for OpenTelemetryPipeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenTelemetryPipeline")
            .field("ready", &self.is_ready())
            .field("shutdown_timeout", &self.shutdown_timeout)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl LifecycleHook for OpenTelemetryPipeline {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "OpenTelemetry flush failed during shutdown".to_owned(),
            })
    }
}

struct HeaderInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let Ok(name) = HeaderName::from_bytes(key.as_bytes()) else {
            return;
        };
        let Ok(value) = HeaderValue::from_str(&value) else {
            return;
        };
        self.0.insert(name, value);
    }
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(HeaderName::as_str).collect()
    }
}

fn composite_propagator() -> TextMapCompositePropagator {
    TextMapCompositePropagator::new(vec![
        Box::new(TraceContextPropagator::new()),
        Box::new(BaggagePropagator::new()),
    ])
}

fn tonic_metadata(headers: &BTreeMap<String, String>) -> Result<tonic::metadata::MetadataMap> {
    let mut metadata = tonic::metadata::MetadataMap::new();
    for (name, value) in headers {
        let key = tonic::metadata::MetadataKey::from_bytes(name.as_bytes()).map_err(|_| {
            OpenTelemetryError::Configuration("invalid gRPC metadata name".to_owned())
        })?;
        let value = tonic::metadata::MetadataValue::try_from(value.as_str()).map_err(|_| {
            OpenTelemetryError::Configuration("invalid gRPC metadata value".to_owned())
        })?;
        metadata.insert(key, value);
    }
    Ok(metadata)
}

fn is_loopback_endpoint(endpoint: &str) -> bool {
    url::Url::parse(endpoint).ok().is_some_and(|url| {
        url.scheme() == "http"
            && match url.host() {
                Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
                Some(url::Host::Ipv4(host)) => host.is_loopback(),
                Some(url::Host::Ipv6(host)) => host.is_loopback(),
                None => false,
            }
    })
}

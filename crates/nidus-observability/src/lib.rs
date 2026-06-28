#![deny(missing_docs)]

//! Production observability composition for Nidus applications.
//!
//! `Observability` composes the lower-level Nidus hooks that already exist for
//! HTTP, events, jobs, tracing, and adapter-owned operations. It does not
//! install process-global exporters or monkey-patch third-party crates.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tracing::Instrument;

#[cfg(feature = "events")]
use nidus_events::{EventObserver, ObservedEventContext};
#[cfg(feature = "http")]
use nidus_http::middleware::{
    ApiDefaults, HttpMetricsHook, MetricsLayer, PrometheusMetrics, metrics_layer,
};
#[cfg(feature = "jobs")]
use nidus_jobs::{JobObserver, JobResultStatus, ObservedJobContext};

/// Production observability configuration and in-process metrics state.
#[derive(Clone, Debug)]
pub struct Observability {
    config: Arc<ObservabilityConfig>,
    state: Arc<Mutex<ObservabilityState>>,
    #[cfg(feature = "http")]
    http_metrics: PrometheusMetrics,
}

impl Observability {
    /// Creates production observability metadata for a service.
    pub fn production(service_name: impl Into<String>) -> Self {
        let service_name = service_name.into();
        Self {
            config: Arc::new(ObservabilityConfig {
                service_name: service_name.clone(),
                version: None,
                environment: None,
                prometheus: false,
                http_metrics: true,
                event_metrics: true,
                job_metrics: true,
                adapter_instrumentation: true,
                tracing: false,
                max_series: None,
                excluded_routes: BTreeSet::new(),
                #[cfg(feature = "otel")]
                otel: None,
            }),
            state: Arc::new(Mutex::new(ObservabilityState::default())),
            #[cfg(feature = "http")]
            http_metrics: PrometheusMetrics::new(),
        }
    }

    /// Sets the service version label used by tracing and OpenTelemetry config.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.config).version = Some(version.into());
        self
    }

    /// Sets the deployment environment label.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.config).environment = Some(environment.into());
        self
    }

    /// Enables Prometheus text exposition for configured metrics.
    pub fn prometheus(mut self) -> Self {
        Arc::make_mut(&mut self.config).prometheus = true;
        self
    }

    /// Enables HTTP tracing when this observability object is applied by Nidus.
    pub fn tracing(mut self) -> Self {
        Arc::make_mut(&mut self.config).tracing = true;
        self
    }

    /// Builds OpenTelemetry resource configuration from environment variables.
    ///
    /// This stores configuration only. Exporters and tracing subscribers remain
    /// explicit application choices.
    #[cfg(feature = "otel")]
    pub fn otel_from_env(mut self) -> Self {
        let mut config = nidus_http::otel::OtelConfig::new(self.service_name());
        if let Some(version) = self.version_label() {
            config = config.version(version);
        }
        if let Some(environment) = self.environment_label() {
            config = config.environment(environment);
        }
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            config = config.with_otlp_endpoint(endpoint);
        }
        Arc::make_mut(&mut self.config).otel = Some(config);
        self
    }

    /// No-op OpenTelemetry setup when the `otel` feature is disabled.
    #[cfg(not(feature = "otel"))]
    pub fn otel_from_env(self) -> Self {
        self
    }

    /// Disables HTTP request metrics.
    pub fn without_http_metrics(mut self) -> Self {
        Arc::make_mut(&mut self.config).http_metrics = false;
        self
    }

    /// Disables observed event metrics.
    pub fn without_event_metrics(mut self) -> Self {
        Arc::make_mut(&mut self.config).event_metrics = false;
        self
    }

    /// Disables observed job metrics.
    pub fn without_job_metrics(mut self) -> Self {
        Arc::make_mut(&mut self.config).job_metrics = false;
        self
    }

    /// Disables adapter-owned operation instrumentation.
    pub fn without_adapter_instrumentation(mut self) -> Self {
        Arc::make_mut(&mut self.config).adapter_instrumentation = false;
        self
    }

    /// Caps distinct low-cardinality labels per metric family.
    pub fn max_series(mut self, max_series: usize) -> Self {
        Arc::make_mut(&mut self.config).max_series = Some(max_series);
        #[cfg(feature = "http")]
        {
            self.http_metrics = self.http_metrics.with_max_series(max_series);
        }
        self
    }

    /// Excludes an HTTP route label from HTTP metrics.
    pub fn exclude_route(mut self, route: impl Into<String>) -> Self {
        let route = route.into();
        Arc::make_mut(&mut self.config)
            .excluded_routes
            .insert(route.clone());
        #[cfg(feature = "http")]
        {
            self.http_metrics = self.http_metrics.exclude_route(route);
        }
        self
    }

    /// Returns the service name.
    pub fn service_name(&self) -> &str {
        &self.config.service_name
    }

    /// Returns the configured service version label.
    pub fn version_label(&self) -> Option<&str> {
        self.config.version.as_deref()
    }

    /// Returns the configured deployment environment label.
    pub fn environment_label(&self) -> Option<&str> {
        self.config.environment.as_deref()
    }

    /// Returns whether HTTP metrics are enabled.
    pub fn http_metrics_enabled(&self) -> bool {
        self.config.prometheus && self.config.http_metrics
    }

    /// Returns whether Prometheus exposition is enabled.
    pub fn prometheus_enabled(&self) -> bool {
        self.config.prometheus
    }

    /// Returns whether HTTP tracing should be applied by Nidus.
    pub fn tracing_enabled(&self) -> bool {
        self.config.tracing
    }

    /// Returns OpenTelemetry resource config when built with `otel`.
    #[cfg(feature = "otel")]
    pub fn otel_config(&self) -> Option<&nidus_http::otel::OtelConfig> {
        self.config.otel.as_ref()
    }

    /// Returns the underlying HTTP Prometheus collector.
    #[cfg(feature = "http")]
    pub fn prometheus_metrics(&self) -> PrometheusMetrics {
        self.http_metrics.clone()
    }

    /// Creates a Tower HTTP metrics layer.
    #[cfg(feature = "http")]
    pub fn http_layer(&self) -> MetricsLayer<ObservabilityHttpMetricsHook> {
        metrics_layer(self.http_metrics_hook())
    }

    /// Creates a `/metrics` router for Prometheus text exposition.
    #[cfg(feature = "http")]
    pub fn routes(&self) -> nidus_http::Router {
        if !self.config.prometheus {
            return nidus_http::Router::new();
        }
        let observability = self.clone();
        nidus_http::Router::new().route(
            "/metrics",
            axum::routing::get(move || {
                let observability = observability.clone();
                async move { observability.render_prometheus() }
            }),
        )
    }

    /// Creates an event observer for [`nidus_events::ObservedEventBus`].
    #[cfg(feature = "events")]
    pub fn event_observer(&self) -> ObservabilityEventObserver {
        ObservabilityEventObserver {
            observability: self.clone(),
        }
    }

    /// Creates a job observer for [`nidus_jobs::ObservedJobRunner`].
    #[cfg(feature = "jobs")]
    pub fn job_observer(&self) -> ObservabilityJobObserver {
        ObservabilityJobObserver {
            observability: self.clone(),
        }
    }

    /// Creates an adapter observer for Nidus-owned adapter operations.
    pub fn adapter_observer(&self) -> ObservabilityAdapterObserver {
        ObservabilityAdapterObserver {
            observability: self.clone(),
        }
    }

    /// Runs a future inside a named tracing span.
    pub async fn instrument<Fut, T>(
        &self,
        operation: impl Into<Cow<'static, str>>,
        future: Fut,
    ) -> T
    where
        Fut: Future<Output = T>,
    {
        let operation = operation.into();
        future
            .instrument(tracing::info_span!(
                "operation",
                otel.name = %operation,
                service.name = %self.service_name(),
                service.version = self.version_label(),
                deployment.environment = self.environment_label()
            ))
            .await
    }

    /// Records module graph validation owned by Nidus application bootstrap.
    pub fn record_module_graph_validation(&self, status: OperationStatus, duration: Duration) {
        let span = tracing::info_span!(
            "module.graph.validate",
            status = status.as_str(),
            duration_ms = duration.as_millis()
        );
        let _entered = span.enter();
        self.record_lifecycle_operation("module.graph.validate", status, duration);
    }

    /// Records application lifecycle operation telemetry.
    pub fn record_lifecycle_operation(
        &self,
        operation: &'static str,
        status: OperationStatus,
        duration: Duration,
    ) {
        let mut state = lock_state(&self.state);
        let operation = state.label("lifecycle", operation.to_owned(), self.config.max_series);
        *state
            .lifecycle_total
            .entry((operation.clone(), status.as_str().to_owned()))
            .or_default() += 1;
        state
            .lifecycle_duration
            .entry((operation, status.as_str().to_owned()))
            .or_default()
            .observe(duration);
    }

    /// Renders all configured Prometheus metrics.
    pub fn render_prometheus(&self) -> String {
        if !self.config.prometheus {
            return String::new();
        }
        let mut output = String::new();
        #[cfg(feature = "http")]
        {
            output.push_str(&self.http_metrics.render());
        }
        let state = lock_state(&self.state).clone();
        output.push_str(&render_observability_metrics(&state));
        output
    }

    #[cfg(feature = "http")]
    fn http_metrics_hook(&self) -> ObservabilityHttpMetricsHook {
        ObservabilityHttpMetricsHook {
            enabled: self.http_metrics_enabled(),
            metrics: self.http_metrics.clone(),
        }
    }

    fn record_event(&self, event_name: &str) {
        if !(self.config.prometheus && self.config.event_metrics) {
            return;
        }
        let mut state = lock_state(&self.state);
        let event_name = state.label("events", event_name.to_owned(), self.config.max_series);
        *state.events_published.entry(event_name).or_default() += 1;
    }

    fn record_job_started(&self, job_name: &'static str) {
        if !(self.config.prometheus && self.config.job_metrics) {
            return;
        }
        let mut state = lock_state(&self.state);
        let job_name = state.label("jobs", job_name.to_owned(), self.config.max_series);
        *state.jobs_started.entry(job_name).or_default() += 1;
    }

    fn record_job_finished(
        &self,
        job_name: &'static str,
        status: JobStatusLabel,
        duration: Option<Duration>,
    ) {
        if !(self.config.prometheus && self.config.job_metrics) {
            return;
        }
        let mut state = lock_state(&self.state);
        let job_name = state.label("jobs", job_name.to_owned(), self.config.max_series);
        let status = status.as_str().to_owned();
        *state
            .jobs_finished
            .entry((job_name.clone(), status.clone()))
            .or_default() += 1;
        if let Some(duration) = duration {
            state
                .job_duration
                .entry((job_name, status))
                .or_default()
                .observe(duration);
        }
    }

    fn record_adapter(
        &self,
        adapter: &'static str,
        operation: &'static str,
        status: OperationStatus,
        duration: Duration,
    ) {
        if !(self.config.prometheus && self.config.adapter_instrumentation) {
            return;
        }
        let span = tracing::info_span!(
            "adapter.operation",
            adapter.name = adapter,
            operation.name = operation,
            status = status.as_str(),
            duration_ms = duration.as_millis()
        );
        let _entered = span.enter();
        let mut state = lock_state(&self.state);
        let series = state.label(
            "adapters",
            format!("{adapter}:{operation}"),
            self.config.max_series,
        );
        let (adapter, operation) = split_adapter_series(&series);
        let status = status.as_str().to_owned();
        *state
            .adapter_operations
            .entry((adapter.clone(), operation.clone(), status.clone()))
            .or_default() += 1;
        state
            .adapter_duration
            .entry((adapter, operation, status))
            .or_default()
            .observe(duration);
    }
}

/// Extension methods that apply [`Observability`] to [`ApiDefaults`].
#[cfg(feature = "http")]
pub trait ApiDefaultsObservabilityExt {
    /// Installs HTTP metrics from an observability object when enabled.
    fn observability(self, observability: &Observability) -> Self;

    /// Applies API defaults and merges observability routes such as `/metrics`.
    fn apply_with_observability(
        self,
        router: nidus_http::Router,
        observability: &Observability,
    ) -> nidus_http::Router;
}

#[cfg(feature = "http")]
impl ApiDefaultsObservabilityExt for ApiDefaults {
    fn observability(self, observability: &Observability) -> Self {
        if observability.http_metrics_enabled() {
            self.metrics(observability.prometheus_metrics())
        } else {
            self
        }
    }

    fn apply_with_observability(
        self,
        router: nidus_http::Router,
        observability: &Observability,
    ) -> nidus_http::Router {
        self.observability(observability)
            .apply(router)
            .merge(observability.routes())
    }
}

#[derive(Clone, Debug)]
struct ObservabilityConfig {
    service_name: String,
    version: Option<String>,
    environment: Option<String>,
    prometheus: bool,
    http_metrics: bool,
    event_metrics: bool,
    job_metrics: bool,
    adapter_instrumentation: bool,
    tracing: bool,
    max_series: Option<usize>,
    excluded_routes: BTreeSet<String>,
    #[cfg(feature = "otel")]
    otel: Option<nidus_http::otel::OtelConfig>,
}

#[derive(Clone, Debug, Default)]
struct ObservabilityState {
    labels: BTreeMap<&'static str, BTreeSet<String>>,
    events_published: BTreeMap<String, u64>,
    jobs_started: BTreeMap<String, u64>,
    jobs_finished: BTreeMap<(String, String), u64>,
    job_duration: BTreeMap<(String, String), DurationHistogram>,
    lifecycle_total: BTreeMap<(String, String), u64>,
    lifecycle_duration: BTreeMap<(String, String), DurationHistogram>,
    adapter_operations: BTreeMap<(String, String, String), u64>,
    adapter_duration: BTreeMap<(String, String, String), DurationHistogram>,
}

impl ObservabilityState {
    fn label(&mut self, family: &'static str, label: String, max_series: Option<usize>) -> String {
        let Some(max_series) = max_series else {
            return label;
        };
        let labels = self.labels.entry(family).or_default();
        if labels.contains(&label) {
            label
        } else if labels.len() < max_series {
            labels.insert(label.clone());
            label
        } else {
            "<overflow>".to_owned()
        }
    }
}

/// HTTP metrics hook returned by [`Observability::http_layer`].
#[cfg(feature = "http")]
#[derive(Clone, Debug)]
pub struct ObservabilityHttpMetricsHook {
    enabled: bool,
    metrics: PrometheusMetrics,
}

#[cfg(feature = "http")]
impl HttpMetricsHook for ObservabilityHttpMetricsHook {
    fn on_request(&self, method: &http::Method, route: Option<&str>) {
        if self.enabled {
            self.metrics.on_request(method, route);
        }
    }

    fn on_response(
        &self,
        method: &http::Method,
        route: Option<&str>,
        status: http::StatusCode,
        latency: Duration,
    ) {
        if self.enabled {
            self.metrics.on_response(method, route, status, latency);
        }
    }

    fn on_error(&self, method: &http::Method, route: Option<&str>, latency: Duration) {
        if self.enabled {
            self.metrics.on_error(method, route, latency);
        }
    }
}

/// Event observer that records low-cardinality event publication metrics.
#[cfg(feature = "events")]
#[derive(Clone, Debug)]
pub struct ObservabilityEventObserver {
    observability: Observability,
}

#[cfg(feature = "events")]
impl<T> EventObserver<T> for ObservabilityEventObserver
where
    T: Clone + Send + Sync + 'static,
{
    fn on_event_published(&self, context: &ObservedEventContext) {
        let span = tracing::info_span!(
            "event.publish",
            event.name = context.event_name(),
            event.operation_id = context.operation_id()
        );
        let _entered = span.enter();
        self.observability.record_event(context.event_name());
    }
}

/// Job observer that records starts, completions, and durations.
#[cfg(feature = "jobs")]
#[derive(Clone, Debug)]
pub struct ObservabilityJobObserver {
    observability: Observability,
}

#[cfg(feature = "jobs")]
impl JobObserver for ObservabilityJobObserver {
    fn on_job_started(&self, context: &ObservedJobContext) {
        self.observability.record_job_started(context.job_name());
    }

    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
        let status = match status {
            JobResultStatus::Success => JobStatusLabel::Success,
            JobResultStatus::Failure => JobStatusLabel::Failure,
        };
        self.observability
            .record_job_finished(context.job_name(), status, context.duration());
    }
}

/// Adapter observer for Nidus-owned adapter operations.
#[derive(Clone, Debug)]
pub struct ObservabilityAdapterObserver {
    observability: Observability,
}

impl ObservabilityAdapterObserver {
    /// Records an adapter-owned operation with a measured duration.
    pub fn record(
        &self,
        adapter: &'static str,
        operation: &'static str,
        status: OperationStatus,
        duration: Duration,
    ) {
        self.observability
            .record_adapter(adapter, operation, status, duration);
    }

    /// Runs a synchronous adapter operation and records success or failure.
    pub fn observe_result<T, E>(
        &self,
        adapter: &'static str,
        operation: &'static str,
        run: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let started_at = Instant::now();
        let result = run();
        self.record(
            adapter,
            operation,
            OperationStatus::from_success(result.is_ok()),
            started_at.elapsed(),
        );
        result
    }
}

/// Operation status label used by lifecycle and adapter metrics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationStatus {
    /// The operation succeeded.
    Success,
    /// The operation failed.
    Failure,
}

impl OperationStatus {
    /// Returns the stable Prometheus label value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }

    /// Creates a status from a boolean success flag.
    pub const fn from_success(ok: bool) -> Self {
        if ok { Self::Success } else { Self::Failure }
    }
}

impl From<bool> for OperationStatus {
    fn from(ok: bool) -> Self {
        Self::from_success(ok)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobStatusLabel {
    Success,
    Failure,
}

impl JobStatusLabel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct DurationHistogram {
    count: u64,
    sum: f64,
    bucket_counts: [u64; DURATION_BUCKETS.len()],
}

impl DurationHistogram {
    fn observe(&mut self, duration: Duration) {
        let seconds = duration.as_secs_f64();
        self.count += 1;
        self.sum += seconds;
        for (bucket, count) in DURATION_BUCKETS.iter().zip(self.bucket_counts.iter_mut()) {
            if seconds <= *bucket {
                *count += 1;
            }
        }
    }
}

const DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000,
];

fn render_observability_metrics(state: &ObservabilityState) -> String {
    let mut output = String::new();
    output.push_str("# TYPE nidus_events_published_total counter\n");
    for (event, count) in &state.events_published {
        output.push_str(&format!(
            "nidus_events_published_total{{event=\"{}\"}} {}\n",
            escape_label(event),
            count
        ));
    }
    output.push_str("# TYPE nidus_jobs_started_total counter\n");
    for (job, count) in &state.jobs_started {
        output.push_str(&format!(
            "nidus_jobs_started_total{{job=\"{}\"}} {}\n",
            escape_label(job),
            count
        ));
    }
    output.push_str("# TYPE nidus_jobs_finished_total counter\n");
    for ((job, status), count) in &state.jobs_finished {
        output.push_str(&format!(
            "nidus_jobs_finished_total{{job=\"{}\",status=\"{}\"}} {}\n",
            escape_label(job),
            escape_label(status),
            count
        ));
    }
    render_histogram(
        &mut output,
        "nidus_job_duration_seconds",
        &["job", "status"],
        state
            .job_duration
            .iter()
            .map(|((job, status), histogram)| (vec![job.as_str(), status.as_str()], histogram)),
    );
    output.push_str("# TYPE nidus_lifecycle_total counter\n");
    for ((operation, status), count) in &state.lifecycle_total {
        output.push_str(&format!(
            "nidus_lifecycle_total{{operation=\"{}\",status=\"{}\"}} {}\n",
            escape_label(operation),
            escape_label(status),
            count
        ));
    }
    render_histogram(
        &mut output,
        "nidus_lifecycle_duration_seconds",
        &["operation", "status"],
        state
            .lifecycle_duration
            .iter()
            .map(|((operation, status), histogram)| {
                (vec![operation.as_str(), status.as_str()], histogram)
            }),
    );
    output.push_str("# TYPE nidus_adapter_operations_total counter\n");
    for ((adapter, operation, status), count) in &state.adapter_operations {
        output.push_str(&format!(
            "nidus_adapter_operations_total{{adapter=\"{}\",operation=\"{}\",status=\"{}\"}} {}\n",
            escape_label(adapter),
            escape_label(operation),
            escape_label(status),
            count
        ));
    }
    render_histogram(
        &mut output,
        "nidus_adapter_operation_duration_seconds",
        &["adapter", "operation", "status"],
        state
            .adapter_duration
            .iter()
            .map(|((adapter, operation, status), histogram)| {
                (
                    vec![adapter.as_str(), operation.as_str(), status.as_str()],
                    histogram,
                )
            }),
    );
    output
}

fn render_histogram<'a>(
    output: &mut String,
    name: &str,
    label_names: &[&str],
    histograms: impl Iterator<Item = (Vec<&'a str>, &'a DurationHistogram)>,
) {
    output.push_str(&format!("# TYPE {name} histogram\n"));
    for (label_values, histogram) in histograms {
        for (bucket, count) in DURATION_BUCKETS.iter().zip(histogram.bucket_counts.iter()) {
            output.push_str(&format!(
                "{name}_bucket{{{},le=\"{}\"}} {}\n",
                render_labels(label_names, &label_values),
                format_bucket(*bucket),
                count
            ));
        }
        output.push_str(&format!(
            "{name}_bucket{{{},le=\"+Inf\"}} {}\n",
            render_labels(label_names, &label_values),
            histogram.count
        ));
        output.push_str(&format!(
            "{name}_count{{{}}} {}\n",
            render_labels(label_names, &label_values),
            histogram.count
        ));
        output.push_str(&format!(
            "{name}_sum{{{}}} {:.6}\n",
            render_labels(label_names, &label_values),
            histogram.sum
        ));
    }
}

fn render_labels(names: &[&str], values: &[&str]) -> String {
    names
        .iter()
        .zip(values.iter())
        .map(|(name, value)| format!("{name}=\"{}\"", escape_label(value)))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_bucket(bucket: f64) -> String {
    format!("{bucket:.3}")
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('"', r#"\""#)
}

fn split_adapter_series(series: &str) -> (String, String) {
    if series == "<overflow>" {
        return ("<overflow>".to_owned(), "<overflow>".to_owned());
    }
    series
        .split_once(':')
        .map(|(adapter, operation)| (adapter.to_owned(), operation.to_owned()))
        .unwrap_or_else(|| (series.to_owned(), "<unknown>".to_owned()))
}

fn lock_state(state: &Mutex<ObservabilityState>) -> std::sync::MutexGuard<'_, ObservabilityState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#![deny(missing_docs)]

//! Production observability composition for Nidus applications.
//!
//! `Observability` composes the lower-level Nidus hooks that already exist for
//! HTTP, events, jobs, tracing, and adapter-owned operations. It does not
//! install process-global exporters or monkey-patch third-party crates.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    future::Future,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

use tracing::Instrument;

#[cfg(feature = "events")]
use nidus_events::{EventBus, EventObserver, ObservedEventBus, ObservedEventContext};
#[cfg(feature = "http")]
use nidus_http::middleware::{
    ApiDefaults, HttpMetricsHook, MetricsLayer, PrometheusMetrics, metrics_layer,
};
#[cfg(feature = "jobs")]
use nidus_jobs::{JobObserver, JobResultStatus, ObservedJobContext, ObservedJobRunner};

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

    /// Creates an observed in-process event bus wired to this observability object.
    #[cfg(feature = "events")]
    pub fn observed_event_bus<T>(&self) -> ObservedEventBus<T, ObservabilityEventObserver>
    where
        T: Clone + Send + Sync + 'static,
    {
        EventBus::new().observed(self.event_observer())
    }

    /// Creates a job observer for [`nidus_jobs::ObservedJobRunner`].
    #[cfg(feature = "jobs")]
    pub fn job_observer(&self) -> ObservabilityJobObserver {
        ObservabilityJobObserver {
            observability: self.clone(),
        }
    }

    /// Creates an observed job runner wired to this observability object.
    #[cfg(feature = "jobs")]
    pub fn job_runner(&self) -> ObservedJobRunner<ObservabilityJobObserver> {
        ObservedJobRunner::new(self.job_observer())
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
        let operation = state.intern_label("lifecycle", operation, self.config.max_series);
        let status = status.as_str();
        *state
            .lifecycle_total
            .entry((Arc::clone(&operation), status))
            .or_default() += 1;
        state
            .lifecycle_duration
            .entry((operation, status))
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
        output.reserve(observability_metrics_capacity(&state));
        render_observability_metrics(&mut output, &state);
        output
    }

    #[cfg(feature = "http")]
    fn http_metrics_hook(&self) -> ObservabilityHttpMetricsHook {
        ObservabilityHttpMetricsHook {
            enabled: self.http_metrics_enabled(),
            metrics: self.http_metrics.clone(),
        }
    }

    #[cfg(feature = "events")]
    fn record_event(&self, event_name: &str) {
        if !(self.config.prometheus && self.config.event_metrics) {
            return;
        }
        let mut state = lock_state(&self.state);
        let event_name = state.intern_label("events", event_name, self.config.max_series);
        *state.events_published.entry(event_name).or_default() += 1;
    }

    #[cfg(feature = "jobs")]
    fn record_job_started(&self, job_name: &'static str) {
        if !(self.config.prometheus && self.config.job_metrics) {
            return;
        }
        let mut state = lock_state(&self.state);
        let job_name = state.intern_label("jobs", job_name, self.config.max_series);
        *state.jobs_started.entry(job_name).or_default() += 1;
    }

    #[cfg(feature = "jobs")]
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
        let job_name = state.intern_label("jobs", job_name, self.config.max_series);
        let status = status.as_str();
        *state
            .jobs_finished
            .entry((Arc::clone(&job_name), status))
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
        let series = state.adapter_series(adapter, operation, self.config.max_series);
        let status = status.as_str();
        *state
            .adapter_operations
            .entry((series, status))
            .or_default() += 1;
        state
            .adapter_duration
            .entry((series, status))
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
    known_labels: BTreeMap<&'static str, BTreeSet<Arc<str>>>,
    known_adapter_series: BTreeSet<AdapterSeries>,
    events_published: BTreeMap<Arc<str>, u64>,
    jobs_started: BTreeMap<Arc<str>, u64>,
    jobs_finished: BTreeMap<(Arc<str>, &'static str), u64>,
    job_duration: BTreeMap<(Arc<str>, &'static str), DurationHistogram>,
    lifecycle_total: BTreeMap<(Arc<str>, &'static str), u64>,
    lifecycle_duration: BTreeMap<(Arc<str>, &'static str), DurationHistogram>,
    adapter_operations: BTreeMap<(AdapterSeries, &'static str), u64>,
    adapter_duration: BTreeMap<(AdapterSeries, &'static str), DurationHistogram>,
}

static OVERFLOW_LABEL: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("<overflow>"));

impl ObservabilityState {
    fn intern_label(
        &mut self,
        family: &'static str,
        label: &str,
        max_series: Option<usize>,
    ) -> Arc<str> {
        let labels = self.known_labels.entry(family).or_default();
        if let Some(label) = labels.get(label) {
            return Arc::clone(label);
        }
        if max_series.is_some_and(|max| labels.len() >= max) {
            return Arc::clone(&OVERFLOW_LABEL);
        }

        let label: Arc<str> = Arc::from(label);
        labels.insert(Arc::clone(&label));
        label
    }

    fn adapter_series(
        &mut self,
        adapter: &'static str,
        operation: &'static str,
        max_series: Option<usize>,
    ) -> AdapterSeries {
        let series = AdapterSeries { adapter, operation };
        if self.known_adapter_series.contains(&series) {
            return series;
        }
        if max_series.is_some_and(|max| self.known_adapter_series.len() >= max) {
            return AdapterSeries::OVERFLOW;
        }

        self.known_adapter_series.insert(series);
        series
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct AdapterSeries {
    adapter: &'static str,
    operation: &'static str,
}

impl AdapterSeries {
    const OVERFLOW: Self = Self {
        adapter: "<overflow>",
        operation: "<overflow>",
    };
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

#[cfg(feature = "jobs")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JobStatusLabel {
    Success,
    Failure,
}

#[cfg(feature = "jobs")]
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

fn render_observability_metrics(output: &mut String, state: &ObservabilityState) {
    output.push_str("# TYPE nidus_events_published_total counter\n");
    for (event, count) in &state.events_published {
        output.push_str("nidus_events_published_total{event=\"");
        write_escaped_label(output, event);
        writeln!(output, "\"}} {count}").expect("writing to a String cannot fail");
    }
    output.push_str("# TYPE nidus_jobs_started_total counter\n");
    for (job, count) in &state.jobs_started {
        output.push_str("nidus_jobs_started_total{job=\"");
        write_escaped_label(output, job);
        writeln!(output, "\"}} {count}").expect("writing to a String cannot fail");
    }
    output.push_str("# TYPE nidus_jobs_finished_total counter\n");
    for ((job, status), count) in &state.jobs_finished {
        output.push_str("nidus_jobs_finished_total{job=\"");
        write_escaped_label(output, job);
        output.push_str("\",status=\"");
        write_escaped_label(output, status);
        writeln!(output, "\"}} {count}").expect("writing to a String cannot fail");
    }
    render_histogram(
        output,
        "nidus_job_duration_seconds",
        state.job_duration.iter().map(|((job, status), histogram)| {
            ([("job", job.as_ref()), ("status", *status)], histogram)
        }),
    );
    output.push_str("# TYPE nidus_lifecycle_total counter\n");
    for ((operation, status), count) in &state.lifecycle_total {
        output.push_str("nidus_lifecycle_total{operation=\"");
        write_escaped_label(output, operation);
        output.push_str("\",status=\"");
        write_escaped_label(output, status);
        writeln!(output, "\"}} {count}").expect("writing to a String cannot fail");
    }
    render_histogram(
        output,
        "nidus_lifecycle_duration_seconds",
        state
            .lifecycle_duration
            .iter()
            .map(|((operation, status), histogram)| {
                (
                    [("operation", operation.as_ref()), ("status", *status)],
                    histogram,
                )
            }),
    );
    output.push_str("# TYPE nidus_adapter_operations_total counter\n");
    for ((series, status), count) in &state.adapter_operations {
        output.push_str("nidus_adapter_operations_total{adapter=\"");
        write_escaped_label(output, series.adapter);
        output.push_str("\",operation=\"");
        write_escaped_label(output, series.operation);
        output.push_str("\",status=\"");
        write_escaped_label(output, status);
        writeln!(output, "\"}} {count}").expect("writing to a String cannot fail");
    }
    render_histogram(
        output,
        "nidus_adapter_operation_duration_seconds",
        state
            .adapter_duration
            .iter()
            .map(|((series, status), histogram)| {
                (
                    [
                        ("adapter", series.adapter),
                        ("operation", series.operation),
                        ("status", *status),
                    ],
                    histogram,
                )
            }),
    );
}

fn observability_metrics_capacity(state: &ObservabilityState) -> usize {
    let counter_series = state
        .events_published
        .len()
        .saturating_add(state.jobs_started.len())
        .saturating_add(state.jobs_finished.len())
        .saturating_add(state.lifecycle_total.len())
        .saturating_add(state.adapter_operations.len());
    let histogram_series = state
        .job_duration
        .len()
        .saturating_add(state.lifecycle_duration.len())
        .saturating_add(state.adapter_duration.len());

    384_usize
        .saturating_add(counter_series.saturating_mul(128))
        .saturating_add(histogram_series.saturating_mul(1_536))
}

fn render_histogram<'a, const N: usize>(
    output: &mut String,
    name: &str,
    histograms: impl Iterator<Item = ([(&'static str, &'a str); N], &'a DurationHistogram)>,
) {
    writeln!(output, "# TYPE {name} histogram").expect("writing to a String cannot fail");
    let mut rendered_labels = String::new();
    for (labels, histogram) in histograms {
        rendered_labels.clear();
        render_labels(&mut rendered_labels, &labels);
        for (bucket, count) in DURATION_BUCKETS.iter().zip(histogram.bucket_counts.iter()) {
            writeln!(
                output,
                "{name}_bucket{{{rendered_labels},le=\"{bucket:.3}\"}} {count}"
            )
            .expect("writing to a String cannot fail");
        }
        writeln!(
            output,
            "{name}_bucket{{{rendered_labels},le=\"+Inf\"}} {}",
            histogram.count
        )
        .expect("writing to a String cannot fail");
        writeln!(
            output,
            "{name}_count{{{rendered_labels}}} {}",
            histogram.count
        )
        .expect("writing to a String cannot fail");
        writeln!(
            output,
            "{name}_sum{{{rendered_labels}}} {:.6}",
            histogram.sum
        )
        .expect("writing to a String cannot fail");
    }
}

fn render_labels<const N: usize>(output: &mut String, labels: &[(&str, &str); N]) {
    for (index, (name, value)) in labels.iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        write!(output, "{name}=\"").expect("writing to a String cannot fail");
        write_escaped_label(output, value);
        output.push('"');
    }
}

fn write_escaped_label(output: &mut String, value: &str) {
    let mut segment_start = 0;
    for (index, character) in value.char_indices() {
        let replacement = match character {
            '\\' => Some(r"\\"),
            '\n' => Some(r"\n"),
            '"' => Some(r#"\""#),
            _ => None,
        };
        if let Some(replacement) = replacement {
            output.push_str(&value[segment_start..index]);
            output.push_str(replacement);
            segment_start = index + character.len_utf8();
        }
    }
    output.push_str(&value[segment_start..]);
}

fn lock_state(state: &Mutex<ObservabilityState>) -> std::sync::MutexGuard<'_, ObservabilityState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::{
        AdapterSeries, DurationHistogram, ObservabilityState, render_observability_metrics,
    };

    #[test]
    fn state_reuses_interned_and_overflow_labels() {
        let mut state = ObservabilityState::default();
        let first = state.intern_label("events", "orders.created", None);
        let repeated = state.intern_label("events", "orders.created", None);
        assert!(Arc::ptr_eq(&first, &repeated));

        let first_overflow = state.intern_label("events", "orders.updated", Some(1));
        let second_overflow = state.intern_label("events", "orders.deleted", Some(1));
        assert_eq!(&*first_overflow, "<overflow>");
        assert!(Arc::ptr_eq(&first_overflow, &second_overflow));
    }

    #[test]
    fn adapter_series_preserve_label_boundaries_and_share_overflow() {
        let mut state = ObservabilityState::default();
        let first = state.adapter_series("adapter:primary", "get", Some(2));
        let second = state.adapter_series("adapter", "primary:get", Some(2));
        assert_ne!(first, second);
        assert_eq!(
            state.adapter_series("adapter:primary", "get", Some(2)),
            first
        );

        let overflow = state.adapter_series("adapter", "set", Some(2));
        assert_eq!(overflow, AdapterSeries::OVERFLOW);
    }

    #[test]
    fn prometheus_renderer_preserves_exact_histogram_and_escape_bytes() {
        let mut state = ObservabilityState::default();
        let series = AdapterSeries {
            adapter: "adapter\\primary",
            operation: "get\"\nline",
        };
        state.adapter_operations.insert((series, "failure"), 7);
        let mut histogram = DurationHistogram::default();
        histogram.observe(Duration::from_millis(12));
        state
            .adapter_duration
            .insert((series, "failure"), histogram);

        let mut rendered = String::new();
        render_observability_metrics(&mut rendered, &state);

        let labels = r#"adapter="adapter\\primary",operation="get\"\nline",status="failure""#;
        let expected = format!(
            concat!(
                "# TYPE nidus_events_published_total counter\n",
                "# TYPE nidus_jobs_started_total counter\n",
                "# TYPE nidus_jobs_finished_total counter\n",
                "# TYPE nidus_job_duration_seconds histogram\n",
                "# TYPE nidus_lifecycle_total counter\n",
                "# TYPE nidus_lifecycle_duration_seconds histogram\n",
                "# TYPE nidus_adapter_operations_total counter\n",
                "nidus_adapter_operations_total{{{labels}}} 7\n",
                "# TYPE nidus_adapter_operation_duration_seconds histogram\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.005\"}} 0\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.010\"}} 0\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.025\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.050\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.100\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.250\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"0.500\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"1.000\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"2.500\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"5.000\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"10.000\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_bucket{{{labels},le=\"+Inf\"}} 1\n",
                "nidus_adapter_operation_duration_seconds_count{{{labels}}} 1\n",
                "nidus_adapter_operation_duration_seconds_sum{{{labels}}} 0.012000\n",
            ),
            labels = labels,
        );

        assert_eq!(rendered, expected);
    }
}

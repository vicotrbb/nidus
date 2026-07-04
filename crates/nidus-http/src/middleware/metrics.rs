use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{Router, routing::get};
use http::{Method, Request, Response, StatusCode};
use tower::{Layer, Service};

/// Creates a metrics hook layer without a stable route label.
///
/// The layer will use Axum's [`axum::extract::MatchedPath`] extension when it is
/// available, otherwise metrics are recorded with route `"<unknown>"`.
pub fn metrics_layer<H>(hook: H) -> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    MetricsLayer::new(hook)
}

/// Creates a metrics hook layer that records a stable route label.
///
/// Use this for route-specific layers when you want stable labels independent
/// of Axum extension timing.
pub fn route_metrics_layer<H>(route: impl Into<Cow<'static, str>>, hook: H) -> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    MetricsLayer::new(hook).route(route)
}

/// Backend-neutral hook for recording HTTP request metrics.
///
/// Implement this trait to bridge Nidus' middleware lifecycle into a concrete
/// metrics backend. Hooks are called in-process: one `on_request` before the
/// inner service, one `on_response` after a response, or `on_error` if the inner
/// service returns an error before producing a response.
pub trait HttpMetricsHook: Clone + Send + Sync + 'static {
    /// Records that a request entered the service.
    fn on_request(&self, method: &Method, route: Option<&str>);

    /// Records that a response left the service.
    fn on_response(
        &self,
        method: &Method,
        route: Option<&str>,
        status: StatusCode,
        latency: Duration,
    );

    /// Records that the inner service returned an error before producing a response.
    fn on_error(&self, _method: &Method, _route: Option<&str>, _latency: Duration) {}
}

/// In-memory Prometheus-format HTTP metrics collector.
///
/// This collector stores counters and bounded duration histograms in process
/// memory and renders Prometheus text exposition. It is useful for small
/// services, examples, and tests; it is not a durable metrics store and values
/// reset on process restart. The default exclusions are `/health/live`,
/// `/health/ready`, and `/metrics`.
///
/// # Label cardinality
///
/// By default the collector records every distinct route label it observes, so
/// the caller is responsible for keeping cardinality bounded. Prefer route
/// patterns (e.g. `"/users/{id}"`) over concrete paths. To harden against
/// accidental high-cardinality labels (which would grow memory without bound in
/// a long-running process), apply [`PrometheusMetrics::with_max_series`]: once
/// the configured number of distinct route labels has been admitted, every
/// further distinct label collapses into a single `"<overflow>"` route.
///
/// ```
/// use axum::{Router, routing::get};
/// use nidus_http::middleware::{PrometheusMetrics, route_metrics_layer};
/// # async fn show_user() -> &'static str { "user" }
///
/// let metrics = PrometheusMetrics::new();
/// let app = Router::new()
///     .route("/users/{id}", get(show_user))
///     .route_layer(route_metrics_layer("/users/{id}", metrics.clone()))
///     .merge(metrics.routes());
/// # let _: Router = app;
/// ```
#[derive(Clone, Debug)]
pub struct PrometheusMetrics {
    state: Arc<Mutex<PrometheusState>>,
    excluded_routes: Arc<BTreeSet<String>>,
    max_series: Option<usize>,
}

impl PrometheusMetrics {
    /// Creates a Prometheus metrics collector with default internal route exclusions.
    ///
    /// The collector is unbounded by default (every distinct route label is
    /// recorded); use [`Self::with_max_series`] to cap label cardinality.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PrometheusState::default())),
            excluded_routes: Arc::new(BTreeSet::from([
                "/health/live".to_owned(),
                "/health/ready".to_owned(),
                "/metrics".to_owned(),
            ])),
            max_series: None,
        }
    }

    /// Adds a route pattern to exclude from recording.
    ///
    /// Match the exact route label emitted by the metrics layer, such as a
    /// static route supplied to [`route_metrics_layer`] or an Axum matched path.
    pub fn exclude_route(mut self, route: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.excluded_routes).insert(route.into());
        self
    }

    /// Bounds the number of distinct route labels retained in memory.
    ///
    /// The first `max_series` distinct route labels are recorded normally; any
    /// further distinct label collapses into a single shared `"<overflow>"`
    /// route. This prevents unbounded memory growth when a layer accidentally
    /// emits high-cardinality labels (for example concrete request paths) while
    /// still keeping the already-admitted routes intact. Without this cap the
    /// collector records every distinct label it observes.
    pub fn with_max_series(mut self, max_series: usize) -> Self {
        self.max_series = Some(max_series);
        self
    }

    /// Creates a metrics layer backed by this collector.
    ///
    /// The layer records request totals, errors, in-flight counts, and bounded
    /// duration histograms. It does not expose a scrape endpoint; use
    /// [`Self::routes`] or [`Self::routes_at`] for that.
    pub fn layer(&self) -> MetricsLayer<Self> {
        MetricsLayer::new(self.clone())
    }

    /// Creates a `/metrics` route for this collector.
    pub fn routes(&self) -> Router {
        self.routes_at("/metrics")
    }

    /// Creates a metrics route at a custom path.
    pub fn routes_at(&self, path: &'static str) -> Router {
        let metrics = self.clone();
        Router::new().route(path, get(move || async move { metrics.render() }))
    }

    /// Renders metrics in Prometheus text exposition format.
    ///
    /// The output includes `nidus_http_requests_total`,
    /// `nidus_http_request_duration_seconds_count`,
    /// `nidus_http_request_duration_seconds_sum`,
    /// `nidus_http_in_flight_requests`, and `nidus_http_errors_total`.
    pub fn render(&self) -> String {
        let state = self.snapshot();
        render_prometheus(&state)
    }

    fn snapshot(&self) -> PrometheusState {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn should_record(&self, route: Option<&str>) -> bool {
        route
            .map(|route| !self.excluded_routes.contains(route))
            .unwrap_or(true)
    }
}

fn render_prometheus(state: &PrometheusState) -> String {
    let mut output = String::new();
    output.push_str("# TYPE nidus_http_requests_total counter\n");
    for ((method, route, status), series) in &state.series {
        output.push_str(&format!(
            "nidus_http_requests_total{{method=\"{}\",route=\"{}\",status=\"{}\"}} {}\n",
            escape_label(method),
            escape_label(route),
            status,
            series.requests
        ));
    }
    output.push_str("# TYPE nidus_http_request_duration_seconds histogram\n");
    for ((method, route, status), series) in &state.series {
        let histogram = &series.histogram;
        for (bucket, count) in HTTP_DURATION_BUCKETS
            .iter()
            .zip(histogram.bucket_counts.iter())
        {
            output.push_str(&format!(
                    "nidus_http_request_duration_seconds_bucket{{method=\"{}\",route=\"{}\",status=\"{}\",le=\"{}\"}} {}\n",
                    escape_label(method),
                    escape_label(route),
                    status,
                    format_bucket(*bucket),
                    count
                ));
        }
        output.push_str(&format!(
                "nidus_http_request_duration_seconds_bucket{{method=\"{}\",route=\"{}\",status=\"{}\",le=\"+Inf\"}} {}\n",
                escape_label(method),
                escape_label(route),
                status,
                histogram.count
            ));
        output.push_str(&format!(
                "nidus_http_request_duration_seconds_count{{method=\"{}\",route=\"{}\",status=\"{}\"}} {}\n",
                escape_label(method),
                escape_label(route),
                status,
                histogram.count
            ));
        output.push_str(&format!(
                "nidus_http_request_duration_seconds_sum{{method=\"{}\",route=\"{}\",status=\"{}\"}} {:.6}\n",
                escape_label(method),
                escape_label(route),
                status,
                histogram.sum
            ));
    }
    output.push_str("# TYPE nidus_http_in_flight_requests gauge\n");
    for ((method, route), count) in &state.in_flight {
        output.push_str(&format!(
            "nidus_http_in_flight_requests{{method=\"{}\",route=\"{}\"}} {}\n",
            escape_label(method),
            escape_label(route),
            count
        ));
    }
    output.push_str("# TYPE nidus_http_errors_total counter\n");
    for ((method, route, status), series) in &state.series {
        if series.errors == 0 {
            continue;
        }
        output.push_str(&format!(
            "nidus_http_errors_total{{method=\"{}\",route=\"{}\",status=\"{}\"}} {}\n",
            escape_label(method),
            escape_label(route),
            status,
            series.errors
        ));
    }
    output
}

impl Default for PrometheusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpMetricsHook for PrometheusMetrics {
    fn on_request(&self, method: &Method, route: Option<&str>) {
        if !self.should_record(route) {
            return;
        }
        let route = route.unwrap_or("<unknown>").to_owned();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let route = match self.max_series {
            Some(max) => state.admit_route(route, max),
            None => route,
        };
        *state
            .in_flight
            .entry((method.as_str().to_owned(), route))
            .or_default() += 1;
    }

    fn on_response(
        &self,
        method: &Method,
        route: Option<&str>,
        status: StatusCode,
        latency: Duration,
    ) {
        let is_error = status.is_client_error() || status.is_server_error();
        self.record_completion(method, route, status.as_u16(), latency, is_error);
    }

    fn on_error(&self, method: &Method, route: Option<&str>, latency: Duration) {
        self.record_completion(
            method,
            route,
            StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            latency,
            true,
        );
    }
}

impl PrometheusMetrics {
    fn record_completion(
        &self,
        method: &Method,
        route: Option<&str>,
        status: u16,
        latency: Duration,
        is_error: bool,
    ) {
        if !self.should_record(route) {
            return;
        }
        let method = method.as_str().to_owned();
        let route = route.unwrap_or("<unknown>").to_owned();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let route = match self.max_series {
            Some(max) => state.admit_route(route, max),
            None => route,
        };
        // Update in-flight first so the owned key can then be moved into the
        // series key without cloning either label.
        let key = (method, route);
        if let Some(count) = state.in_flight.get_mut(&key) {
            *count = count.saturating_sub(1);
        }
        let (method, route) = key;
        let series = state.series.entry((method, route, status)).or_default();
        series.requests += 1;
        series.histogram.observe(latency);
        if is_error {
            series.errors += 1;
        }
    }
}

#[derive(Clone, Debug, Default)]
struct PrometheusState {
    series: BTreeMap<(String, String, u16), StatusSeries>,
    in_flight: BTreeMap<(String, String), u64>,
    known_routes: BTreeSet<String>,
}

/// Counters and histogram for one `(method, route, status)` label set.
#[derive(Clone, Debug, Default)]
struct StatusSeries {
    requests: u64,
    errors: u64,
    histogram: DurationHistogram,
}

impl PrometheusState {
    /// Returns the label to record for `route`, honoring a cap on the number of
    /// distinct route labels. Already-admitted routes are returned unchanged;
    /// once the cap is reached, new labels collapse to `"<overflow>"`. Callers
    /// with no cap must skip this call entirely (the uncapped path pays nothing).
    fn admit_route(&mut self, route: String, max_series: usize) -> String {
        if self.known_routes.contains(&route) {
            route
        } else if self.known_routes.len() < max_series {
            self.known_routes.insert(route.clone());
            route
        } else {
            "<overflow>".to_owned()
        }
    }
}

const HTTP_DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.000, 2.500, 5.000, 10.000,
];

#[derive(Clone, Debug, Default)]
struct DurationHistogram {
    count: u64,
    sum: f64,
    bucket_counts: [u64; HTTP_DURATION_BUCKETS.len()],
}

impl DurationHistogram {
    fn observe(&mut self, latency: Duration) {
        let seconds = latency.as_secs_f64();
        self.count += 1;
        self.sum += seconds;
        for (bucket, count) in HTTP_DURATION_BUCKETS
            .iter()
            .zip(self.bucket_counts.iter_mut())
        {
            if seconds <= *bucket {
                *count += 1;
            }
        }
    }
}

/// Tower layer that invokes [`HttpMetricsHook`] for request lifecycle metrics.
///
/// Route labels come from [`Self::route`] when set, then from Axum
/// [`axum::extract::MatchedPath`], and finally `"<unknown>"`.
#[derive(Clone, Debug)]
pub struct MetricsLayer<H> {
    hook: H,
    route: Option<Cow<'static, str>>,
}

impl<H> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    /// Creates a metrics layer without a route label.
    pub fn new(hook: H) -> Self {
        Self { hook, route: None }
    }

    /// Adds a stable route label to emitted metrics.
    ///
    /// Prefer route patterns such as `"/users/:id"` over concrete paths to keep
    /// label cardinality bounded.
    pub fn route(mut self, route: impl Into<Cow<'static, str>>) -> Self {
        self.route = Some(route.into());
        self
    }
}

impl<S, H> Layer<S> for MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    type Service = MetricsService<S, H>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsService {
            inner,
            hook: self.hook.clone(),
            route: self.route.clone(),
        }
    }
}

/// Service produced by [`MetricsLayer`].
#[derive(Clone, Debug)]
pub struct MetricsService<S, H> {
    inner: S,
    hook: H,
    route: Option<Cow<'static, str>>,
}

impl<S, H, RequestBody, ResponseBody> Service<Request<RequestBody>> for MetricsService<S, H>
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    H: HttpMetricsHook,
    RequestBody: Send + 'static,
    ResponseBody: Send + 'static,
{
    type Response = Response<ResponseBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<RequestBody>) -> Self::Future {
        let method = request.method().clone();
        let hook = self.hook.clone();
        let route = self.route.clone().or_else(|| {
            request
                .extensions()
                .get::<axum::extract::MatchedPath>()
                .map(|path| Cow::Owned(path.as_str().to_owned()))
        });
        hook.on_request(&method, route.as_deref());
        let started_at = Instant::now();
        let future = self.inner.call(request);

        Box::pin(async move {
            match future.await {
                Ok(response) => {
                    hook.on_response(
                        &method,
                        route.as_deref(),
                        response.status(),
                        started_at.elapsed(),
                    );
                    Ok(response)
                }
                Err(error) => {
                    hook.on_error(&method, route.as_deref(), started_at.elapsed());
                    Err(error)
                }
            }
        })
    }
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('"', r#"\""#)
}

fn format_bucket(bucket: f64) -> String {
    if bucket.fract() == 0.0 {
        format!("{bucket:.0}")
    } else {
        let formatted = format!("{bucket:.3}");
        formatted.trim_end_matches('0').to_owned()
    }
}

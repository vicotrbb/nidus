//! Tower middleware helpers.

use std::{
    borrow::Cow,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use http::{HeaderValue, Method, Request, Response, StatusCode, header::HeaderName};
use nidus_core::{Container, RequestScope, SharedRequestScope};
use tower::{Layer, Service, limit::RateLimitLayer, timeout::TimeoutLayer};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{HttpMakeClassifier, MakeSpan, TraceLayer};
use tracing::{Level, Span};

/// Creates a Tower timeout layer.
pub fn timeout_layer(timeout: Duration) -> TimeoutLayer {
    TimeoutLayer::new(timeout)
}

/// Creates a Tower rate limit layer.
pub fn rate_limit_layer(num: u64, per: Duration) -> RateLimitLayer {
    RateLimitLayer::new(num, per)
}

/// Creates a response request-id layer.
pub fn request_id_layer() -> RequestIdLayer {
    RequestIdLayer
}

/// Creates a request-scope layer backed by a shared dependency container.
pub fn request_scope_layer(container: Arc<Container>) -> RequestScopeLayer {
    RequestScopeLayer::new(container)
}

/// Creates a metrics hook layer without a stable route label.
pub fn metrics_layer<H>(hook: H) -> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    MetricsLayer::new(hook)
}

/// Creates a metrics hook layer that records a stable route label.
pub fn route_metrics_layer<H>(route: impl Into<Cow<'static, str>>, hook: H) -> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    MetricsLayer::new(hook).route(route)
}

/// Creates a permissive CORS layer for API development and examples.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

/// Creates a gzip response compression layer.
pub fn compression_layer() -> CompressionLayer {
    CompressionLayer::new()
}

/// Creates an HTTP tracing layer for requests and responses.
pub fn trace_layer() -> TraceLayer<HttpMakeClassifier> {
    TraceLayer::new_for_http()
}

/// Creates an HTTP tracing layer that records a stable route label.
pub fn route_trace_layer(
    route: impl Into<Cow<'static, str>>,
) -> TraceLayer<HttpMakeClassifier, RouteMakeSpan> {
    TraceLayer::new_for_http().make_span_with(RouteMakeSpan::new(route))
}

/// Tower layer that creates one dependency request scope per HTTP request.
#[derive(Clone)]
pub struct RequestScopeLayer {
    container: Arc<Container>,
}

impl RequestScopeLayer {
    /// Creates a request-scope layer backed by a shared dependency container.
    pub fn new(container: Arc<Container>) -> Self {
        Self { container }
    }
}

impl<S> Layer<S> for RequestScopeLayer {
    type Service = RequestScopeService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestScopeService {
            inner,
            container: Arc::clone(&self.container),
        }
    }
}

/// Service produced by [`RequestScopeLayer`].
#[derive(Clone)]
pub struct RequestScopeService<S> {
    inner: S,
    container: Arc<Container>,
}

impl<S, RequestBody> Service<Request<RequestBody>> for RequestScopeService<S>
where
    S: Service<Request<RequestBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    RequestBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<RequestBody>) -> Self::Future {
        let scope: SharedRequestScope = Arc::new(RequestScope::from_shared_container(Arc::clone(
            &self.container,
        )));
        request.extensions_mut().insert(scope);
        self.inner.call(request)
    }
}

/// Span maker that records request method, URI, and a stable route label.
#[derive(Clone, Debug)]
pub struct RouteMakeSpan {
    route: Cow<'static, str>,
}

impl RouteMakeSpan {
    /// Creates a route-labelled span maker.
    pub fn new(route: impl Into<Cow<'static, str>>) -> Self {
        Self {
            route: route.into(),
        }
    }
}

impl<B> MakeSpan<B> for RouteMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        tracing::span!(
            Level::DEBUG,
            "request",
            method = %request.method(),
            uri = %request.uri(),
            route = %self.route,
        )
    }
}

/// Backend-neutral hook for recording HTTP request metrics.
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

/// Tower layer that invokes [`HttpMetricsHook`] for request lifecycle metrics.
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
        let route = self.route.clone();
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

/// Tower layer that adds an `x-request-id` response header when absent.
///
/// Incoming request IDs are propagated to the response unless the inner service
/// already set a response ID. Requests without an ID receive a generated one.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// Service produced by [`RequestIdLayer`].
#[derive(Clone, Debug)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S, RequestBody, ResponseBody> Service<Request<RequestBody>> for RequestIdService<S>
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
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
        let request_id = request.headers().get(request_id_header()).cloned();
        let future = self.inner.call(request);
        Box::pin(async move {
            let mut response = future.await?;
            response
                .headers_mut()
                .entry(request_id_header())
                .or_insert_with(|| request_id.unwrap_or_else(new_request_id));
            Ok(response)
        })
    }
}

fn request_id_header() -> HeaderName {
    HeaderName::from_static("x-request-id")
}

fn new_request_id() -> HeaderValue {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    HeaderValue::from_str(&format!("nidus-{nanos}"))
        .expect("generated request id contains only valid header characters")
}

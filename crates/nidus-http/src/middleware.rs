//! Tower middleware helpers.

use std::{
    borrow::Cow,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use http::{Method, Request};
use nidus_core::{Container, RequestScope, SharedRequestScope};
use tower::{Layer, Service, limit::RateLimitLayer, timeout::TimeoutLayer};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{HttpMakeClassifier, MakeSpan, TraceLayer};
use tracing::{Level, Span};

mod metrics;
mod request_id;

pub use metrics::{
    HttpMetricsHook, MetricsLayer, MetricsService, metrics_layer, route_metrics_layer,
};
pub use request_id::{RequestIdLayer, RequestIdService};

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

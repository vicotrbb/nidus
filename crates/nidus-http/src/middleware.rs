//! Tower middleware helpers.

use std::{borrow::Cow, time::Duration};

use http::{Method, Request};
use tower::{limit::RateLimitLayer, timeout::TimeoutLayer};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{HttpMakeClassifier, MakeSpan, TraceLayer};
use tracing::{Level, Span};

mod metrics;
mod request_id;
mod request_scope;

pub use metrics::{
    HttpMetricsHook, MetricsLayer, MetricsService, metrics_layer, route_metrics_layer,
};
pub use request_id::{RequestIdLayer, RequestIdService};
pub use request_scope::{RequestScopeLayer, RequestScopeService, request_scope_layer};

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

//! Tower middleware helpers.

use std::{borrow::Cow, time::Duration};

use http::{HeaderValue, Method, Request};
use tower::{limit::RateLimitLayer as TowerRateLimitLayer, timeout::TimeoutLayer};
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{HttpMakeClassifier, MakeSpan, TraceLayer};
use tracing::{Level, Span};

mod api_defaults;
mod metrics;
mod rate_limit;
mod request_context;
mod request_id;
mod request_scope;
mod security;

pub use crate::context::{
    ClientKind, IdentityExtractor, RequestContext, RequestIdentity, api_key_identity,
    client_ip_identity, context_identity,
};
pub use api_defaults::ApiDefaults;
pub use metrics::{
    HttpMetricsHook, MetricsLayer, MetricsService, PrometheusMetrics, metrics_layer,
    route_metrics_layer,
};
pub use rate_limit::{
    InMemoryRateLimitStore, RateLimitConfig, RateLimitDecision, RateLimitError, RateLimitLayer,
    RateLimitService, RateLimitStore,
};
pub use request_context::{RequestContextLayer, RequestContextService, request_context_layer};
pub use request_id::{
    RequestIdConfig, RequestIdLayer, RequestIdMode, RequestIdPolicy, RequestIdService,
    ValidatedRequestIdLayer, ValidatedRequestIdService, validated_request_id_layer,
};
pub use request_scope::{RequestScopeLayer, RequestScopeService, request_scope_layer};
pub use security::{
    BodyLimitLayer, BodyLimitService, SecurityHeadersLayer, SecurityHeadersService,
    TimeoutResponseLayer, TimeoutResponseService, body_limit_layer, security_headers_layer,
    streaming_body_limit_layer, timeout_response_layer, webhook_body_limit_layer,
};

/// Creates a Tower timeout layer.
pub fn timeout_layer(timeout: Duration) -> TimeoutLayer {
    TimeoutLayer::new(timeout)
}

/// Creates a Tower rate limit layer.
pub fn rate_limit_layer(num: u64, per: Duration) -> TowerRateLimitLayer {
    TowerRateLimitLayer::new(num, per)
}

/// Creates the legacy response-only request-id layer.
///
/// This helper mirrors any inbound `x-request-id` to the response or generates
/// a `nidus-<timestamp>` value when absent. It does not validate IDs or insert
/// [`RequestContext`]. Use [`validated_request_id_layer`] for production UUID
/// v4 request IDs and request context population.
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

/// Creates a CORS layer for a single explicit API origin.
pub fn cors_origin_layer(origin: HeaderValue) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(origin)
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

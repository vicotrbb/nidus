use std::time::Duration;

use axum::Router;

use crate::{
    error::ErrorEnvelopeLayer,
    health::HealthRegistry,
    middleware::{
        PrometheusMetrics, RateLimitConfig, RequestIdConfig, body_limit_layer, catch_panic_layer,
        request_context_layer, security_headers_layer, streaming_body_limit_layer,
        timeout_response_layer, validated_request_id_layer,
    },
};

/// High-level configurable API defaults built from explicit Axum/Tower primitives.
///
/// `ApiDefaults` is a convenience builder for production-oriented middleware,
/// not a hidden application runtime. [`ApiDefaults::production`] starts with
/// request IDs, request context, production error envelopes, health routes,
/// security headers, a `Content-Length` body limit, and a timeout enabled.
/// Prometheus metrics and rate limiting are opt-in and only run when configured.
///
/// `version` and `environment` are stored as labels on this builder for callers
/// that want to keep one deployment metadata object, but [`ApiDefaults::apply`]
/// does not currently emit those labels to logs, metrics, headers, or health
/// responses.
///
/// ```
/// use axum::{Router, routing::get};
/// use nidus_http::{
///     health::{HealthRegistry, HealthStatus},
///     middleware::{ApiDefaults, PrometheusMetrics, RequestIdConfig},
/// };
/// # async fn list_users() -> &'static str { "users" }
///
/// let metrics = PrometheusMetrics::new();
/// let health = HealthRegistry::new()
///     .ready_check_sync("database", || HealthStatus::up());
///
/// let router = Router::new().route("/users", get(list_users));
/// let app = ApiDefaults::production("users-api")
///     .metrics(metrics.clone())
///     .health(health)
///     .request_ids(RequestIdConfig::production())
///     .apply(router)
///     .merge(metrics.routes());
/// # let _: Router = app;
/// ```
#[derive(Clone)]
pub struct ApiDefaults {
    service_name: String,
    version: Option<String>,
    environment: Option<String>,
    request_ids: Option<RequestIdConfig>,
    request_context: bool,
    error_envelope: bool,
    metrics: Option<PrometheusMetrics>,
    health: Option<HealthRegistry>,
    rate_limit: Option<RateLimitConfig>,
    security_headers: bool,
    body_limit: Option<u64>,
    streaming_body_limit: Option<usize>,
    timeout: Option<Duration>,
    catch_panic: bool,
}

impl ApiDefaults {
    /// Creates production defaults for a service.
    ///
    /// Enabled by default:
    /// - request IDs: [`RequestIdConfig::production`], which requires inbound
    ///   IDs to be UUID v4 and generates UUID v4 IDs when absent
    /// - request context: [`request_context_layer`]
    /// - error responses: [`ErrorEnvelopeLayer`]
    /// - health routes: [`HealthRegistry::new`] at `/health/live` and
    ///   `/health/ready`
    /// - security headers: [`security_headers_layer`]
    /// - body limit: [`body_limit_layer`] with `1 MiB`
    /// - timeout: [`timeout_response_layer`] with `30s`
    /// - panic catching: [`catch_panic_layer`] so a panicking handler yields a
    ///   `500` envelope instead of aborting the connection
    ///
    /// Metrics and rate limiting are disabled unless [`Self::metrics`] or
    /// [`Self::rate_limit`] is called. The metrics middleware records requests,
    /// but `apply` does not merge the `/metrics` route; merge
    /// [`PrometheusMetrics::routes`] yourself when you want it exposed.
    pub fn production(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            version: None,
            environment: None,
            request_ids: Some(RequestIdConfig::production()),
            request_context: true,
            error_envelope: true,
            metrics: None,
            health: Some(HealthRegistry::new()),
            rate_limit: None,
            security_headers: true,
            body_limit: Some(1024 * 1024),
            streaming_body_limit: None,
            timeout: Some(Duration::from_secs(30)),
            catch_panic: true,
        }
    }

    /// Returns the service name attached to these defaults.
    ///
    /// The current [`Self::apply`] implementation keeps this as builder metadata
    /// only; it is not emitted by any default middleware.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Sets a service version label.
    ///
    /// This is metadata on the builder. [`Self::apply`] does not currently
    /// attach the version to metrics, health responses, logs, or response
    /// headers.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Sets an environment label.
    ///
    /// This is metadata on the builder. [`Self::apply`] does not currently
    /// attach the environment to metrics, health responses, logs, or response
    /// headers.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Replaces request ID behavior.
    ///
    /// Pass [`RequestIdConfig::development`] for permissive inbound validation
    /// during local development, or a custom config when you need a different
    /// header name or generator.
    pub fn request_ids(mut self, config: RequestIdConfig) -> Self {
        self.request_ids = Some(config);
        self
    }

    /// Disables request ID middleware.
    pub fn without_request_ids(mut self) -> Self {
        self.request_ids = None;
        self
    }

    /// Disables request context middleware.
    pub fn without_request_context(mut self) -> Self {
        self.request_context = false;
        self
    }

    /// Disables production error envelopes.
    pub fn without_error_envelope(mut self) -> Self {
        self.error_envelope = false;
        self
    }

    /// Adds a Prometheus metrics collector.
    ///
    /// This installs request lifecycle recording. It does not expose the
    /// collector's `/metrics` route; merge [`PrometheusMetrics::routes`] into
    /// the router when you want scrape output.
    pub fn metrics(mut self, metrics: PrometheusMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Disables metrics middleware.
    pub fn without_metrics(mut self) -> Self {
        self.metrics = None;
        self
    }

    /// Replaces health routes.
    ///
    /// The registry contributes `/health/live` and `/health/ready` routes before
    /// middleware layers are applied, so the same default security, timeout, and
    /// body/header handling applies to health responses too.
    pub fn health(mut self, health: HealthRegistry) -> Self {
        self.health = Some(health);
        self
    }

    /// Disables health route helpers.
    pub fn without_health(mut self) -> Self {
        self.health = None;
        self
    }

    /// Adds rate limiting.
    pub fn rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit = Some(config);
        self
    }

    /// Disables rate limiting.
    pub fn without_rate_limit(mut self) -> Self {
        self.rate_limit = None;
        self
    }

    /// Enables or replaces the request body size limit.
    ///
    /// The built-in layer checks the declared `Content-Length` header only. It
    /// rejects declared oversized bodies with `413 Payload Too Large`; it does
    /// not count streamed bytes when the header is absent or invalid (e.g.
    /// chunked-transfer clients). For a hard read-time cap across streaming
    /// bodies, also enable [`Self::streaming_body_limit`].
    pub fn body_limit(mut self, max_bytes: u64) -> Self {
        self.body_limit = Some(max_bytes);
        self
    }

    /// Enables a streaming request body limit that counts bytes as they are read.
    ///
    /// Unlike [`Self::body_limit`] (which inspects only the declared
    /// `Content-Length`), this wraps the request body and enforces `max_bytes`
    /// even when `Content-Length` is absent, closing the chunked-transfer
    /// bypass. The cap is applied as the downstream extractor or handler reads
    /// the body, so a request is rejected only once it actually reads past the
    /// limit. This is opt-in because it wraps every request body; pair it with
    /// [`Self::body_limit`] for an early `Content-Length` rejection plus a hard
    /// streaming cap.
    pub fn streaming_body_limit(mut self, max_bytes: usize) -> Self {
        self.streaming_body_limit = Some(max_bytes);
        self
    }

    /// Disables request body size limiting.
    pub fn without_body_limit(mut self) -> Self {
        self.body_limit = None;
        self
    }

    /// Enables response security headers.
    pub fn security_headers(mut self) -> Self {
        self.security_headers = true;
        self
    }

    /// Disables response security headers.
    pub fn without_security_headers(mut self) -> Self {
        self.security_headers = false;
        self
    }

    /// Sets a default request timeout.
    ///
    /// Requests whose inner service does not finish before this duration receive
    /// `408 Request Timeout` with a plain-text `request timed out` body.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Disables timeout middleware.
    pub fn without_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Disables the panic-catching layer.
    ///
    /// With it disabled, a panicking handler may abort the connection instead of
    /// yielding the production `500` envelope. It is enabled by
    /// [`Self::production`].
    pub fn without_catch_panic(mut self) -> Self {
        self.catch_panic = false;
        self
    }

    /// Applies the configured defaults to an existing router.
    ///
    /// Health routes are merged first. The effective inbound request order for
    /// the default production stack is (outermost first):
    ///
    /// 1. [`security_headers_layer`] response wrapper
    /// 2. [`validated_request_id_layer`]
    /// 3. [`request_context_layer`]
    /// 4. metrics, when configured
    /// 5. [`ErrorEnvelopeLayer`]
    /// 6. [`timeout_response_layer`]
    /// 7. [`body_limit_layer`] `Content-Length` boundary
    /// 8. rate limiting, when configured
    /// 9. [`catch_panic_layer`], when enabled (innermost, a handler panic is
    ///    caught and surfaced as a `500` through every outer layer)
    /// 10. route handlers
    ///
    /// `body_limit` sits inside the request-id, metrics, and error-envelope
    /// layers so an oversized-body `413` is enveloped, metered, and carries a
    /// request id (consistent with how `408` timeouts are observed), rather than
    /// being rejected invisibly at the edge.
    ///
    /// Order matters when adding route-specific layers. Layers installed on a
    /// route before calling `apply` run inside these defaults, so they can see
    /// the validated request ID and enriched [`crate::context::RequestContext`],
    /// and their error responses can be wrapped by the production envelope.
    pub fn apply(self, mut router: Router) -> Router {
        if let Some(health) = self.health {
            router = router.merge(health.routes());
        }
        // Innermost layer: catch handler panics so they surface as an enveloped
        // 500 through every outer layer instead of aborting the connection.
        if self.catch_panic {
            router = router.layer(catch_panic_layer());
        }
        if let Some(rate_limit) = self.rate_limit {
            router = router.layer(rate_limit.layer());
        }
        if let Some(max_bytes) = self.body_limit {
            router = router.layer(body_limit_layer(max_bytes));
        }
        if let Some(max_bytes) = self.streaming_body_limit {
            router = router.layer(streaming_body_limit_layer(max_bytes));
        }
        if let Some(timeout) = self.timeout {
            router = router.layer(timeout_response_layer(timeout));
        }
        if self.error_envelope {
            router = router.layer(ErrorEnvelopeLayer::new());
        }
        if let Some(metrics) = self.metrics {
            router = router.layer(metrics.layer());
        }
        if self.request_context {
            router = router.layer(request_context_layer());
        }
        if let Some(request_ids) = self.request_ids {
            router = router.layer(validated_request_id_layer(request_ids));
        }
        if self.security_headers {
            router = router.layer(security_headers_layer());
        }
        router
    }
}

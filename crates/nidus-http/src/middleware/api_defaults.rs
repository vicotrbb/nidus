use std::time::Duration;

use axum::Router;

use crate::{
    error::ErrorEnvelopeLayer,
    health::HealthRegistry,
    middleware::{
        PrometheusMetrics, RateLimitConfig, RequestIdConfig, body_limit_layer,
        request_context_layer, security_headers_layer, timeout_response_layer,
        validated_request_id_layer,
    },
};

/// High-level configurable API defaults built from explicit Axum/Tower primitives.
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
    timeout: Option<Duration>,
}

impl ApiDefaults {
    /// Creates production defaults for a service.
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
            timeout: Some(Duration::from_secs(30)),
        }
    }

    /// Returns the service name attached to these defaults.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Sets a service version label.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Sets an environment label.
    pub fn environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Replaces request ID behavior.
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
    pub fn body_limit(mut self, max_bytes: u64) -> Self {
        self.body_limit = Some(max_bytes);
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
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Disables timeout middleware.
    pub fn without_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Applies the configured defaults to an existing router.
    pub fn apply(self, mut router: Router) -> Router {
        if let Some(health) = self.health {
            router = router.merge(health.routes());
        }
        if let Some(rate_limit) = self.rate_limit {
            router = router.layer(rate_limit.layer());
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
        if let Some(max_bytes) = self.body_limit {
            router = router.layer(body_limit_layer(max_bytes));
        }
        if self.security_headers {
            router = router.layer(security_headers_layer());
        }
        router
    }
}

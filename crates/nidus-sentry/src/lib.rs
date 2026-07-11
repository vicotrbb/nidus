#![deny(missing_docs)]

//! First-party Sentry integration for Nidus.
//!
//! This crate owns initialization, panic/error/tracing capture, request-local
//! Tower hubs, matched-route transactions, secure event scrubbing, bounded
//! duplicate suppression, and graceful off-runtime flushing.

use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use http::Request;
use nidus_core::{Container, LifecycleHook, NidusError};
use sentry::{
    Client, ClientInitGuard, ClientOptions, Hub, Level, MaxRequestBodySize,
    protocol::{Breadcrumb, Event},
};
use sentry_tower::{HubProvider, SentryHttpLayer, SentryLayer};
use tower::Layer;
use tracing_subscriber::registry::LookupSpan;

const MAX_DEDUP_ENTRIES: usize = 16_384;

/// Result type for Sentry integration operations.
pub type Result<T> = std::result::Result<T, SentryError>;

/// Error returned by Sentry configuration or lifecycle operations.
#[derive(Debug, thiserror::Error)]
pub enum SentryError {
    /// Configuration is invalid or unsafe.
    #[error("invalid Sentry configuration: {0}")]
    Configuration(String),
    /// A blocking flush task failed to join.
    #[error("Sentry lifecycle task failed: {0}")]
    TaskJoin(String),
    /// Sentry did not drain its bounded transport queue before the deadline.
    #[error("Sentry transport did not flush before the configured timeout")]
    FlushTimeout,
}

/// Redaction-safe Sentry client configuration.
#[derive(Clone)]
pub struct SentryConfig {
    dsn: String,
    release: Option<String>,
    environment: Option<String>,
    event_sample_rate: f32,
    trace_sample_rate: f32,
    shutdown_timeout: Duration,
    dedup_window: Duration,
    dedup_capacity: usize,
    allow_insecure_local_dsn: bool,
}

impl SentryConfig {
    /// Creates a secure configuration from an explicit DSN.
    pub fn new(dsn: impl Into<String>) -> Self {
        Self {
            dsn: dsn.into(),
            release: None,
            environment: None,
            event_sample_rate: 1.0,
            trace_sample_rate: 0.1,
            shutdown_timeout: Duration::from_secs(2),
            dedup_window: Duration::from_secs(5),
            dedup_capacity: 2_048,
            allow_insecure_local_dsn: false,
        }
    }

    /// Loads the DSN from `SENTRY_DSN` without exposing it in diagnostics.
    pub fn from_env() -> Result<Self> {
        let dsn = std::env::var("SENTRY_DSN")
            .map_err(|_| SentryError::Configuration("SENTRY_DSN is required".to_owned()))?;
        Ok(Self::new(dsn))
    }

    /// Sets the release identifier.
    pub fn with_release(mut self, release: impl Into<String>) -> Self {
        self.release = Some(release.into());
        self
    }

    /// Sets the deployment environment.
    pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
        self.environment = Some(environment.into());
        self
    }

    /// Sets error-event and performance-trace sample rates.
    pub fn with_sample_rates(mut self, events: f32, traces: f32) -> Result<Self> {
        if !events.is_finite()
            || !traces.is_finite()
            || !(0.0..=1.0).contains(&events)
            || !(0.0..=1.0).contains(&traces)
        {
            return Err(SentryError::Configuration(
                "sample rates must be between 0 and 1".to_owned(),
            ));
        }
        self.event_sample_rate = events;
        self.trace_sample_rate = traces;
        Ok(self)
    }

    /// Sets transport shutdown and flushing timeout.
    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(SentryError::Configuration(
                "shutdown timeout must be between 1 millisecond and 60 seconds".to_owned(),
            ));
        }
        self.shutdown_timeout = timeout;
        Ok(self)
    }

    /// Sets bounded in-process duplicate suppression.
    pub fn with_deduplication(mut self, window: Duration, capacity: usize) -> Result<Self> {
        if window.is_zero()
            || window > Duration::from_secs(300)
            || capacity == 0
            || capacity > MAX_DEDUP_ENTRIES
        {
            return Err(SentryError::Configuration(
                "deduplication requires a 1ms..=300s window and 1..=16384 entries".to_owned(),
            ));
        }
        self.dedup_window = window;
        self.dedup_capacity = capacity;
        Ok(self)
    }

    /// Allows HTTP only for an explicit loopback Sentry DSN.
    pub fn allow_insecure_local_dsn(mut self) -> Result<Self> {
        let dsn = parse_dsn(&self.dsn)?;
        if dsn.scheme().to_string() != "http" || !is_loopback_host(dsn.host()) {
            return Err(SentryError::Configuration(
                "insecure Sentry DSNs are restricted to loopback".to_owned(),
            ));
        }
        self.allow_insecure_local_dsn = true;
        Ok(self)
    }

    /// Returns the configured release.
    pub fn release(&self) -> Option<&str> {
        self.release.as_deref()
    }

    /// Returns the configured environment.
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    fn validate(&self) -> Result<sentry::types::Dsn> {
        let dsn = parse_dsn(&self.dsn)?;
        let scheme = dsn.scheme().to_string();
        if scheme != "https"
            && !(scheme == "http" && self.allow_insecure_local_dsn && is_loopback_host(dsn.host()))
        {
            return Err(SentryError::Configuration(
                "Sentry DSN must use HTTPS unless loopback HTTP is explicitly enabled".to_owned(),
            ));
        }
        Ok(dsn)
    }

    fn client_options(&self) -> Result<ClientOptions> {
        let dsn = self.validate()?;
        let deduplicator = Arc::new(EventDeduplicator::new(
            self.dedup_window,
            self.dedup_capacity,
        ));
        let before_send_deduplicator = Arc::clone(&deduplicator);

        let mut options = ClientOptions::new()
            .sample_rate(self.event_sample_rate)
            .traces_sample_rate(self.trace_sample_rate)
            .send_default_pii(false)
            .attach_stacktrace(true)
            .shutdown_timeout(self.shutdown_timeout)
            .max_breadcrumbs(100)
            .max_request_body_size(MaxRequestBodySize::None)
            .before_send(move |event| {
                let event = redact_event(event);
                (!before_send_deduplicator.is_duplicate(&event)).then_some(event)
            })
            .before_breadcrumb(|mut breadcrumb| {
                redact_breadcrumb(&mut breadcrumb);
                Some(breadcrumb)
            });
        options.dsn = Some(dsn);
        options.release = self.release.clone().map(Into::into);
        options.environment = self.environment.clone().map(Into::into);
        options.accept_invalid_certs = false;
        options.send_default_pii = false;
        options.enable_logs = false;
        Ok(options)
    }
}

impl fmt::Debug for SentryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SentryConfig")
            .field("dsn", &"<redacted>")
            .field("release", &self.release)
            .field("environment", &self.environment)
            .field("event_sample_rate", &self.event_sample_rate)
            .field("trace_sample_rate", &self.trace_sample_rate)
            .field("shutdown_timeout", &self.shutdown_timeout)
            .field("dedup_window", &self.dedup_window)
            .field("dedup_capacity", &self.dedup_capacity)
            .field("allow_insecure_local_dsn", &self.allow_insecure_local_dsn)
            .finish()
    }
}

/// Initialized Sentry client with owned global restoration and lifecycle state.
#[derive(Clone)]
pub struct SentryIntegration {
    inner: Arc<SentryInner>,
}

struct SentryInner {
    hub: Arc<Hub>,
    previous_client: Option<Arc<Client>>,
    client: Arc<Client>,
    guard: Mutex<Option<ClientInitGuard>>,
    shutdown_timeout: Duration,
    shutdown: AtomicBool,
}

impl SentryIntegration {
    /// Initializes the official Sentry SDK and its panic integration.
    ///
    /// Keep the returned value in application lifecycle state so queued events
    /// are flushed and the previous hub client is restored on shutdown.
    pub fn init(config: SentryConfig) -> Result<Self> {
        let options = config.client_options()?;
        let hub = Hub::current();
        let previous_client = hub.client();
        let guard = sentry::init(options);
        let client = hub.client().ok_or_else(|| {
            SentryError::Configuration(
                "Sentry client was not bound after initialization".to_owned(),
            )
        })?;
        Ok(Self {
            inner: Arc::new(SentryInner {
                hub,
                previous_client,
                client,
                guard: Mutex::new(Some(guard)),
                shutdown_timeout: config.shutdown_timeout,
                shutdown: AtomicBool::new(false),
            }),
        })
    }

    /// Returns the native Sentry client.
    pub fn client(&self) -> &Arc<Client> {
        &self.inner.client
    }

    /// Returns the native base hub used to create isolated request hubs.
    pub fn hub(&self) -> &Arc<Hub> {
        &self.inner.hub
    }

    /// Registers this integration as a typed singleton dependency.
    pub fn register(&self, container: &mut Container) -> nidus_core::Result<()> {
        container.register_singleton(self.clone())
    }

    /// Creates the correctly ordered Tower layer for request-hub isolation,
    /// redacted request capture, distributed tracing, and matched-route names.
    pub fn tower_layer<B>(&self) -> SentryTowerLayer<B> {
        SentryTowerLayer::new(Arc::clone(&self.inner.hub))
    }

    /// Creates a Sentry `tracing_subscriber` layer.
    ///
    /// Errors become Sentry events, warning/info events become breadcrumbs, and
    /// info-or-higher spans participate in performance traces.
    pub fn tracing_layer<S>(&self) -> sentry_tracing::SentryLayer<S>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span>,
    {
        sentry_tracing::layer()
            .event_filter(|metadata| match *metadata.level() {
                tracing::Level::ERROR => sentry_tracing::EventFilter::Event,
                tracing::Level::WARN | tracing::Level::INFO => {
                    sentry_tracing::EventFilter::Breadcrumb
                }
                tracing::Level::DEBUG | tracing::Level::TRACE => {
                    sentry_tracing::EventFilter::Ignore
                }
            })
            .span_filter(|metadata| {
                matches!(
                    *metadata.level(),
                    tracing::Level::ERROR | tracing::Level::WARN | tracing::Level::INFO
                )
            })
    }

    /// Captures a standard Rust error through this integration's native hub.
    pub fn capture_error<E>(&self, error: &E) -> sentry::types::Uuid
    where
        E: Error + ?Sized,
    {
        self.inner
            .hub
            .capture_event(sentry::event_from_error(error))
    }

    /// Captures a redaction-safe message at a chosen severity.
    pub fn capture_message(&self, message: &str, level: Level) -> sentry::types::Uuid {
        self.inner.hub.capture_message(message, level)
    }

    /// Returns whether the client is enabled and has not been shut down.
    pub fn is_ready(&self) -> bool {
        self.inner.client.is_enabled() && !self.inner.shutdown.load(Ordering::Acquire)
    }

    /// Adds Sentry client lifecycle state as a Nidus readiness check.
    #[cfg(feature = "health")]
    pub fn register_ready_check(
        self: Arc<Self>,
        registry: nidus_http::health::HealthRegistry,
        name: impl Into<String>,
    ) -> nidus_http::health::HealthRegistry {
        registry.ready_check(name, move || {
            let integration = Arc::clone(&self);
            async move {
                if integration.is_ready() {
                    nidus_http::health::HealthStatus::up()
                } else {
                    nidus_http::health::HealthStatus::down("Sentry client is not ready")
                }
            }
        })
    }

    /// Records redaction-safe client readiness in the Nidus dashboard timeline.
    #[cfg(feature = "dashboard")]
    pub async fn record_dashboard_status(
        &self,
        collector: &nidus_dashboard::DashboardCollector<
            nidus_dashboard::storage::DashboardStorageHandle,
        >,
    ) -> nidus_dashboard::Result<()> {
        collector
            .record_adapter("nidus-sentry.readiness", None, self.is_ready(), 0)
            .await
    }

    /// Flushes queued events on a blocking worker rather than the async runtime.
    pub async fn flush(&self) -> Result<()> {
        if self.inner.shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        let client = Arc::clone(&self.inner.client);
        let timeout = self.inner.shutdown_timeout;
        let flushed = tokio::task::spawn_blocking(move || client.flush(Some(timeout)))
            .await
            .map_err(|error| SentryError::TaskJoin(error.to_string()))?;
        if flushed {
            Ok(())
        } else {
            Err(SentryError::FlushTimeout)
        }
    }

    /// Flushes, closes the transport, and restores the previous hub client once.
    pub async fn shutdown(&self) -> Result<()> {
        if self
            .inner
            .shutdown
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(());
        }

        let guard = lock(&self.inner.guard).take();
        let hub = Arc::clone(&self.inner.hub);
        let previous_client = self.inner.previous_client.clone();
        let client = Arc::clone(&self.inner.client);
        let timeout = self.inner.shutdown_timeout;
        let flushed = tokio::task::spawn_blocking(move || {
            let flushed = client.flush(Some(timeout));
            hub.bind_client(previous_client);
            drop(guard);
            flushed
        })
        .await
        .map_err(|error| SentryError::TaskJoin(error.to_string()))?;
        if flushed {
            Ok(())
        } else {
            Err(SentryError::FlushTimeout)
        }
    }
}

impl fmt::Debug for SentryIntegration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SentryIntegration")
            .field("enabled", &self.inner.client.is_enabled())
            .field("ready", &self.is_ready())
            .field("shutdown_timeout", &self.inner.shutdown_timeout)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl LifecycleHook for SentryIntegration {
    async fn on_shutdown(&self) -> nidus_core::Result<()> {
        self.shutdown()
            .await
            .map_err(|_| NidusError::ApplicationBuild {
                message: "Sentry flush failed during shutdown".to_owned(),
            })
    }
}

/// Provider that derives a fresh request hub from a fixed base hub.
#[derive(Clone)]
pub struct IsolatedHubProvider {
    base: Arc<Hub>,
}

impl<B> HubProvider<Arc<Hub>, Request<B>> for IsolatedHubProvider {
    fn hub(&self, _request: &Request<B>) -> Arc<Hub> {
        Arc::new(Hub::new_from_top(&self.base))
    }
}

/// Correctly ordered Tower layer for Sentry request isolation and HTTP traces.
pub struct SentryTowerLayer<B> {
    provider: IsolatedHubProvider,
    http: SentryHttpLayer,
    body: PhantomData<fn() -> B>,
}

impl<B> Clone for SentryTowerLayer<B> {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            http: self.http.clone(),
            body: PhantomData,
        }
    }
}

impl<B> SentryTowerLayer<B> {
    /// Creates a layer derived from a native base hub.
    pub fn new(base: Arc<Hub>) -> Self {
        Self {
            provider: IsolatedHubProvider { base },
            http: SentryHttpLayer::new().enable_transaction(),
            body: PhantomData,
        }
    }
}

impl<S, B> Layer<S> for SentryTowerLayer<B> {
    type Service = sentry_tower::SentryService<
        sentry_tower::SentryHttpService<S>,
        IsolatedHubProvider,
        Arc<Hub>,
        Request<B>,
    >;

    fn layer(&self, service: S) -> Self::Service {
        let http = self.http.layer(service);
        SentryLayer::new(self.provider.clone()).layer(http)
    }
}

/// Removes request bodies, query strings, cookies, users, secrets, and common
/// PII fields from an event before transport.
///
/// Free-form error messages and exception values remain useful for diagnosis;
/// applications must never place credentials in those strings.
pub fn redact_event(mut event: Event<'static>) -> Event<'static> {
    event.user = None;
    event.server_name = None;
    event.extra.clear();
    event.tags.retain(|key, _| !is_sensitive_key(key));
    if let Some(request) = &mut event.request {
        request.data = None;
        request.query_string = None;
        request.cookies = None;
        request.env.clear();
        request.headers.retain(|name, _| safe_header(name));
        if let Some(url) = &mut request.url {
            url.set_query(None);
            url.set_fragment(None);
            let _ = url.set_username("");
            let _ = url.set_password(None);
        }
    }
    for breadcrumb in event.breadcrumbs.iter_mut() {
        redact_breadcrumb(breadcrumb);
    }
    event
}

fn redact_breadcrumb(breadcrumb: &mut Breadcrumb) {
    breadcrumb.data.clear();
}

struct EventDeduplicator {
    window: Duration,
    capacity: usize,
    state: Mutex<HashMap<u64, Instant>>,
}

impl EventDeduplicator {
    fn new(window: Duration, capacity: usize) -> Self {
        Self {
            window,
            capacity,
            state: Mutex::new(HashMap::new()),
        }
    }

    fn is_duplicate(&self, event: &Event<'_>) -> bool {
        let now = Instant::now();
        let fingerprint = event_fingerprint(event);
        let mut state = lock(&self.state);
        state.retain(|_, seen_at| now.saturating_duration_since(*seen_at) <= self.window);
        if state.contains_key(&fingerprint) {
            return true;
        }
        if state.len() >= self.capacity
            && let Some(oldest) = state
                .iter()
                .min_by_key(|(_, seen_at)| **seen_at)
                .map(|(key, _)| *key)
        {
            state.remove(&oldest);
        }
        state.insert(fingerprint, now);
        false
    }
}

fn event_fingerprint(event: &Event<'_>) -> u64 {
    let mut hasher = DefaultHasher::new();
    event.level.hash(&mut hasher);
    event.message.hash(&mut hasher);
    event.transaction.hash(&mut hasher);
    event.logger.hash(&mut hasher);
    for exception in event.exception.iter() {
        exception.ty.hash(&mut hasher);
        exception.value.hash(&mut hasher);
        exception.module.hash(&mut hasher);
    }
    for fingerprint in event.fingerprint.iter() {
        fingerprint.hash(&mut hasher);
    }
    hasher.finish()
}

fn safe_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "accept"
            | "content-length"
            | "content-type"
            | "host"
            | "traceparent"
            | "tracestate"
            | "sentry-trace"
            | "x-request-id"
    )
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "password",
        "secret",
        "token",
        "api_key",
        "apikey",
        "session",
        "email",
        "username",
        "ip_address",
    ]
    .iter()
    .any(|sensitive| key.contains(sensitive))
}

fn parse_dsn(value: &str) -> Result<sentry::types::Dsn> {
    value
        .parse()
        .map_err(|_| SentryError::Configuration("Sentry DSN could not be parsed".to_owned()))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn before_send_redacts_and_deduplicates_events() {
        let options = SentryConfig::new("https://public@sentry.invalid/1")
            .client_options()
            .unwrap();
        let events = sentry::test::with_captured_events_options(
            || {
                for _ in 0..2 {
                    let mut request = sentry::protocol::Request {
                        url: Some(
                            "https://alice:password@example.test/orders?token=secret"
                                .parse()
                                .unwrap(),
                        ),
                        query_string: Some("token=secret".to_owned()),
                        cookies: Some("session=secret".to_owned()),
                        data: Some("credit_card=secret".to_owned()),
                        ..Default::default()
                    };
                    request
                        .headers
                        .insert("authorization".to_owned(), "Bearer secret".to_owned());
                    request
                        .headers
                        .insert("content-type".to_owned(), "application/json".to_owned());
                    request
                        .headers
                        .insert("baggage".to_owned(), "tenant.email=alice".to_owned());
                    let mut event = Event {
                        message: Some("database unavailable".to_owned()),
                        request: Some(request),
                        user: Some(sentry::protocol::User {
                            email: Some("alice@example.test".to_owned()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    };
                    event
                        .tags
                        .insert("api_token".to_owned(), "secret".to_owned());
                    event.extra.insert(
                        "payload".to_owned(),
                        sentry::protocol::Value::String("secret".to_owned()),
                    );
                    sentry::capture_event(event);
                }
            },
            options,
        );

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(event.user.is_none());
        assert!(event.extra.is_empty());
        assert!(!event.tags.contains_key("api_token"));
        let request = event.request.as_ref().unwrap();
        assert!(request.data.is_none());
        assert!(request.query_string.is_none());
        assert!(request.cookies.is_none());
        assert!(!request.headers.contains_key("authorization"));
        assert!(!request.headers.contains_key("baggage"));
        assert_eq!(
            request.headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
        let url = request.url.as_ref().unwrap();
        assert!(url.query().is_none());
        assert!(url.username().is_empty());
        assert!(url.password().is_none());
    }
}

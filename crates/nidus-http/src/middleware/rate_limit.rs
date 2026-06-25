use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use axum::{body::Body, extract::Request};
use http::{HeaderValue, Response, StatusCode};
use tower::{Layer, Service};

use crate::context::{IdentityExtractor, RequestIdentity};

type IdentityFn =
    Arc<dyn Fn(&http::request::Parts) -> Option<RequestIdentity> + Send + Sync + 'static>;

/// Error returned by rate-limit stores.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct RateLimitError {
    message: String,
}

impl RateLimitError {
    /// Creates a rate-limit store error.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Decision returned by a rate-limit store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitDecision {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Limit for the active window.
    pub limit: u64,
    /// Remaining requests in the active window.
    pub remaining: u64,
    /// Seconds until the window resets.
    pub reset_after: Duration,
}

/// Store adapter used by rate-limit layers.
pub trait RateLimitStore: Send + Sync + 'static {
    /// Checks and consumes one request for an identity.
    fn check(
        &self,
        identity: &RequestIdentity,
        limit: u64,
        window: Duration,
    ) -> Result<RateLimitDecision, RateLimitError>;
}

/// In-memory rate-limit store intended for local development and single-process apps.
///
/// The store tracks counters in process memory and opportunistically removes
/// expired identity windows whenever [`RateLimitStore::check`] runs. It is not
/// shared across processes, not durable across restarts, and not a substitute
/// for a distributed limiter at multi-instance production boundaries.
#[derive(Clone, Default)]
pub struct InMemoryRateLimitStore {
    state: Arc<Mutex<HashMap<String, WindowState>>>,
}

impl InMemoryRateLimitStore {
    /// Creates an empty in-memory rate-limit store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of identity windows currently retained by the store.
    ///
    /// Expired windows are pruned opportunistically during [`RateLimitStore::check`],
    /// so this value is mainly useful for tests, diagnostics, and local tools.
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .map(|state| state.len())
            .unwrap_or_default()
    }

    /// Returns whether the store currently retains no identity windows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl RateLimitStore for InMemoryRateLimitStore {
    fn check(
        &self,
        identity: &RequestIdentity,
        limit: u64,
        window: Duration,
    ) -> Result<RateLimitDecision, RateLimitError> {
        let now = Instant::now();
        let mut state = self
            .state
            .lock()
            .map_err(|_| RateLimitError::new("rate limit store poisoned"))?;
        state.retain(|_, window_state| now.duration_since(window_state.started_at) < window);
        let window_state = state
            .entry(identity.as_str().to_owned())
            .or_insert_with(|| WindowState {
                started_at: now,
                count: 0,
            });
        if now.duration_since(window_state.started_at) >= window {
            window_state.started_at = now;
            window_state.count = 0;
        }

        let allowed = window_state.count < limit;
        if allowed {
            window_state.count += 1;
        }
        let remaining = limit.saturating_sub(window_state.count);
        let reset_after = window.saturating_sub(now.duration_since(window_state.started_at));
        Ok(RateLimitDecision {
            allowed,
            limit,
            remaining,
            reset_after,
        })
    }
}

#[derive(Clone)]
struct WindowState {
    started_at: Instant,
    count: u64,
}

/// Typed config for production-shaped rate limiting.
#[derive(Clone)]
pub struct RateLimitConfig {
    limit: u64,
    window: Duration,
    store: Arc<dyn RateLimitStore>,
    identity: IdentityFn,
    fail_open: bool,
}

impl RateLimitConfig {
    /// Creates a rate-limit config with an explicit store.
    pub fn new(limit: u64, window: Duration, store: impl RateLimitStore) -> Self {
        Self {
            limit,
            window,
            store: Arc::new(store),
            identity: Arc::new(|_parts| Some(RequestIdentity::new("anonymous"))),
            fail_open: true,
        }
    }

    /// Replaces the identity extractor.
    pub fn identity(mut self, extractor: impl IdentityExtractor) -> Self {
        self.identity = Arc::new(move |parts| extractor.extract(parts));
        self
    }

    /// Allows requests when the backing store fails.
    pub fn fail_open(mut self) -> Self {
        self.fail_open = true;
        self
    }

    /// Rejects requests when the backing store fails.
    pub fn fail_closed(mut self) -> Self {
        self.fail_open = false;
        self
    }

    /// Creates a Tower layer from this config.
    pub fn layer(self) -> RateLimitLayer {
        RateLimitLayer { config: self }
    }
}

/// Tower layer that applies configured rate limiting.
#[derive(Clone)]
pub struct RateLimitLayer {
    config: RateLimitConfig,
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimitService {
            inner,
            config: self.config.clone(),
        }
    }
}

/// Service produced by [`RateLimitLayer`].
#[derive(Clone)]
pub struct RateLimitService<S> {
    inner: S,
    config: RateLimitConfig,
}

impl<S> Service<Request> for RateLimitService<S>
where
    S: Service<Request, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let config = self.config.clone();
        let (parts, body) = request.into_parts();
        let identity =
            (config.identity)(&parts).unwrap_or_else(|| RequestIdentity::new("anonymous"));
        let decision = config
            .store
            .check(&identity, config.limit, config.window)
            .unwrap_or(RateLimitDecision {
                allowed: config.fail_open,
                limit: config.limit,
                remaining: if config.fail_open { config.limit } else { 0 },
                reset_after: config.window,
            });

        if !decision.allowed {
            return Box::pin(async move { Ok(rate_limited_response(decision)) });
        }

        let future = self.inner.call(Request::from_parts(parts, body));
        Box::pin(async move {
            let mut response = future.await?;
            insert_rate_limit_headers(response.headers_mut(), &decision);
            Ok(response)
        })
    }
}

fn rate_limited_response(decision: RateLimitDecision) -> Response<Body> {
    let mut response = Response::new(Body::from("rate limit exceeded"));
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    insert_rate_limit_headers(response.headers_mut(), &decision);
    response.headers_mut().insert(
        http::header::RETRY_AFTER,
        HeaderValue::from_str(&decision.reset_after.as_secs().max(1).to_string())
            .expect("retry-after must be a valid header"),
    );
    response
}

fn insert_rate_limit_headers(headers: &mut http::HeaderMap, decision: &RateLimitDecision) {
    headers.insert(
        "ratelimit-limit",
        HeaderValue::from_str(&decision.limit.to_string()).expect("limit header must be valid"),
    );
    headers.insert(
        "ratelimit-remaining",
        HeaderValue::from_str(&decision.remaining.to_string())
            .expect("remaining header must be valid"),
    );
    headers.insert(
        "ratelimit-reset",
        HeaderValue::from_str(&decision.reset_after.as_secs().max(1).to_string())
            .expect("reset header must be valid"),
    );
}

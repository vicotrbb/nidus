#![deny(missing_docs)]

//! Authentication and guard support.

mod middleware;

use async_trait::async_trait;
use axum::{Json, response::IntoResponse};
use http::{
    HeaderMap, StatusCode,
    header::{AUTHORIZATION, AsHeaderName},
};
use serde::Serialize;
use std::sync::Arc;

pub use middleware::{GuardLayer, GuardService, guard_layer};

/// Composable authorization guard.
#[async_trait]
pub trait Guard<S>: Send + Sync + 'static {
    /// Checks whether the request context is authorized.
    async fn check(&self, ctx: GuardContext<S>) -> Result<(), GuardError>;
}

#[async_trait]
impl<S, G> Guard<S> for Arc<G>
where
    S: Send + Sync + 'static,
    G: Guard<S>,
{
    async fn check(&self, ctx: GuardContext<S>) -> Result<(), GuardError> {
        self.as_ref().check(ctx).await
    }
}

/// Extension methods for composing guards.
pub trait GuardExt<S>: Guard<S> + Sized {
    /// Requires both guards to authorize the request.
    fn and<G>(self, other: G) -> AndGuard<Self, G>
    where
        G: Guard<S>,
    {
        AndGuard {
            first: self,
            second: other,
        }
    }

    /// Requires at least one guard to authorize the request.
    fn or<G>(self, other: G) -> OrGuard<Self, G>
    where
        G: Guard<S>,
    {
        OrGuard {
            first: self,
            second: other,
        }
    }
}

impl<S, G> GuardExt<S> for G where G: Guard<S> + Sized {}

/// Guard that succeeds only when both inner guards succeed.
#[derive(Clone, Debug)]
pub struct AndGuard<A, B> {
    first: A,
    second: B,
}

#[async_trait]
impl<S, A, B> Guard<S> for AndGuard<A, B>
where
    S: Clone + Send + Sync + 'static,
    A: Guard<S>,
    B: Guard<S>,
{
    async fn check(&self, ctx: GuardContext<S>) -> Result<(), GuardError> {
        self.first.check(ctx.clone()).await?;
        self.second.check(ctx).await
    }
}

/// Guard that succeeds when either inner guard succeeds.
#[derive(Clone, Debug)]
pub struct OrGuard<A, B> {
    first: A,
    second: B,
}

#[async_trait]
impl<S, A, B> Guard<S> for OrGuard<A, B>
where
    S: Clone + Send + Sync + 'static,
    A: Guard<S>,
    B: Guard<S>,
{
    async fn check(&self, ctx: GuardContext<S>) -> Result<(), GuardError> {
        let first_error = match self.first.check(ctx.clone()).await {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        match self.second.check(ctx).await {
            Ok(()) => Ok(()),
            Err(_) => Err(first_error),
        }
    }
}

/// Typed guard context.
#[derive(Clone, Debug)]
pub struct GuardContext<S> {
    state: S,
    route_label: Arc<str>,
    headers: HeaderMap,
}

impl<S> GuardContext<S> {
    /// Creates a guard context with typed state and a route label.
    pub fn new(state: S, route_label: impl Into<String>) -> Self {
        Self {
            state,
            route_label: Arc::from(route_label.into()),
            headers: HeaderMap::new(),
        }
    }

    /// Creates a context from an already shared immutable route label.
    ///
    /// This constructor supports framework-generated handlers that retain one
    /// route label for the lifetime of a service. Most application code should
    /// use [`Self::new`].
    #[doc(hidden)]
    pub fn from_shared_route_label(state: S, route_label: Arc<str>) -> Self {
        Self {
            state,
            route_label,
            headers: HeaderMap::new(),
        }
    }

    /// Returns typed state available to the guard.
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Returns the route label being authorized.
    pub fn route_label(&self) -> &str {
        &self.route_label
    }

    /// Returns request headers available to HTTP guards.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a request header as a UTF-8 string.
    ///
    /// Missing headers return `Ok(None)`. Present headers that are not valid
    /// UTF-8 return `401 Unauthorized` so guards do not accidentally authorize
    /// malformed credentials.
    pub fn header_str<K>(&self, name: K) -> Result<Option<&str>>
    where
        K: AsHeaderName,
    {
        self.headers
            .get(name)
            .map(|value| {
                value
                    .to_str()
                    .map_err(|_| GuardError::unauthorized("header value is not valid UTF-8"))
            })
            .transpose()
    }

    /// Returns the `Authorization: Bearer <token>` token when present.
    ///
    /// A missing authorization header returns `Ok(None)`. A present header with
    /// another scheme, missing token, or whitespace inside the token returns
    /// `401 Unauthorized`. The authentication scheme is matched
    /// case-insensitively, as required by HTTP semantics.
    pub fn bearer_token(&self) -> Result<Option<&str>> {
        let Some(header) = self.header_str(AUTHORIZATION)? else {
            return Ok(None);
        };
        let Some((scheme, token)) = header.split_once(' ') else {
            return Err(GuardError::unauthorized(
                "authorization header must use `Bearer <token>`",
            ));
        };
        if !scheme.eq_ignore_ascii_case("Bearer")
            || token.is_empty()
            || token.chars().any(char::is_whitespace)
        {
            return Err(GuardError::unauthorized(
                "authorization header must use `Bearer <token>`",
            ));
        }
        Ok(Some(token))
    }

    /// Returns an API key from an explicit header as a UTF-8 string.
    ///
    /// This helper intentionally does not compare secrets. Use constant-time
    /// comparison or a server-side secret store in application code when the
    /// key value is sensitive.
    pub fn api_key<K>(&self, header_name: K) -> Result<Option<&str>>
    where
        K: AsHeaderName,
    {
        self.header_str(header_name)
    }

    /// Attaches request headers to this guard context.
    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    /// Consumes the context and returns its state.
    pub fn into_state(self) -> S {
        self.state
    }
}

/// Result type for guards.
pub type Result<T, E = GuardError> = std::result::Result<T, E>;

/// Authorization failure.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{reason}")]
pub struct GuardError {
    status_code: StatusCode,
    reason: String,
}

impl GuardError {
    /// Creates a guard error with an explicit HTTP status and reason.
    pub fn new(status_code: StatusCode, reason: impl Into<String>) -> Self {
        Self {
            status_code,
            reason: reason.into(),
        }
    }

    /// Creates a 401 authorization error.
    pub fn unauthorized(reason: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, reason)
    }

    /// Creates a 403 authorization error.
    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, reason)
    }

    /// Returns the HTTP status code corresponding to this guard failure.
    pub fn status_code(&self) -> StatusCode {
        self.status_code
    }

    /// Returns the authorization failure reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        match self.status_code {
            StatusCode::UNAUTHORIZED => "unauthorized",
            StatusCode::FORBIDDEN => "forbidden",
            _ => "authorization_failed",
        }
    }
}

impl IntoResponse for GuardError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code;
        let body = Json(GuardErrorBody {
            error: GuardErrorDetails {
                code: self.code(),
                message: self.reason,
            },
        });
        (status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct GuardErrorBody {
    error: GuardErrorDetails,
}

#[derive(Debug, Serialize)]
struct GuardErrorDetails {
    code: &'static str,
    message: String,
}

//! Authentication and guard support.

use async_trait::async_trait;
use http::StatusCode;

/// Composable authorization guard.
#[async_trait]
pub trait Guard<S>: Send + Sync + 'static {
    /// Checks whether the request context is authorized.
    async fn check(&self, ctx: GuardContext<S>) -> Result<(), GuardError>;
}

/// Typed guard context.
#[derive(Clone, Debug)]
pub struct GuardContext<S> {
    state: S,
    route_label: String,
}

impl<S> GuardContext<S> {
    /// Creates a guard context with typed state and a route label.
    pub fn new(state: S, route_label: impl Into<String>) -> Self {
        Self {
            state,
            route_label: route_label.into(),
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
    /// Creates a 401 authorization error.
    pub fn unauthorized(reason: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::UNAUTHORIZED,
            reason: reason.into(),
        }
    }

    /// Creates a 403 authorization error.
    pub fn forbidden(reason: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::FORBIDDEN,
            reason: reason.into(),
        }
    }

    /// Returns the HTTP status code corresponding to this guard failure.
    pub fn status_code(&self) -> StatusCode {
        self.status_code
    }

    /// Returns the authorization failure reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

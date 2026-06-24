//! Authentication and guard support.

use async_trait::async_trait;
use axum::{Json, response::IntoResponse};
use http::StatusCode;
use serde::Serialize;

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

//! HTTP error helpers.

use axum::{Json, response::IntoResponse};
use http::StatusCode;
use serde::Serialize;

/// HTTP error response with stable client-facing JSON shape.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct HttpError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl HttpError {
    /// Creates an HTTP error with an explicit status, code, and message.
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// Creates a 400 bad request error.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// Creates a 401 unauthorized error.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    /// Creates a 403 forbidden error.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// Creates a 404 not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// Creates a 409 conflict error.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// Creates a 429 too many requests error.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "too_many_requests", message)
    }

    /// Creates a 422 unprocessable entity error.
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unprocessable_entity",
            message,
        )
    }

    /// Creates a sanitized 500 internal server error.
    pub fn internal_server_error() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_server_error",
            "internal server error",
        )
    }

    /// Returns the HTTP status code.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the client-facing error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl IntoResponse for HttpError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status;
        let body = Json(ErrorBody {
            error: ErrorDetails {
                code: self.code,
                message: self.message,
            },
        });
        (status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorDetails,
}

#[derive(Debug, Serialize)]
struct ErrorDetails {
    code: &'static str,
    message: String,
}

/// Invalid route path declared through the manual HTTP routing API.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("route path `{path}` contains a parameter segment without a name after ':'")]
pub struct RoutePathError {
    path: String,
}

impl RoutePathError {
    /// Creates an error for a route path parameter segment without a name.
    pub fn empty_parameter(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the invalid route path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

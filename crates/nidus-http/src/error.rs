//! HTTP error helpers.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{Json, body::Body, body::to_bytes, extract::Request, response::IntoResponse};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tower::{Layer, Service};

use crate::context::RequestContext;

/// HTTP error response with stable client-facing JSON shape.
///
/// `HttpError` constructors produce the legacy/simple body
/// `{ "error": { "code": "...", "message": "..." } }`. When
/// [`ErrorEnvelopeLayer`] is installed, that body is wrapped into the
/// production envelope with status, timestamp, path, and request ID fields.
///
/// Client-error constructors such as [`Self::bad_request`] and
/// [`Self::not_found`] expose the message you provide. Use
/// [`Self::internal_server_error`] for 500 responses that must not leak
/// implementation details; the production envelope also masks any 5xx response
/// message to `"internal server error"` and clears `details`.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct HttpError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl HttpError {
    /// Creates an HTTP error with an explicit status, code, and message.
    ///
    /// For non-5xx statuses, the message is client-facing. For 5xx statuses,
    /// prefer [`Self::internal_server_error`] unless the response is guaranteed
    /// to be safe; [`ErrorEnvelopeLayer`] will still mask 5xx messages.
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    /// Creates a 400 bad request error with a client-facing message.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    /// Creates a 401 unauthorized error with a client-facing message.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    /// Creates a 403 forbidden error with a client-facing message.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    /// Creates a 404 not found error with a client-facing message.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    /// Creates a 409 conflict error with a client-facing message.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// Creates a 429 too many requests error with a client-facing message.
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "too_many_requests", message)
    }

    /// Creates a 422 unprocessable entity error with a client-facing message.
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unprocessable_entity",
            message,
        )
    }

    /// Creates a sanitized 500 internal server error.
    ///
    /// The message is always `"internal server error"` so callers do not
    /// accidentally expose database errors, stack traces, or upstream payloads.
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
        let code = self.code;
        let message = self.message;

        if status.is_server_error() {
            tracing::error!(
                http.status = status.as_u16(),
                error.code = code,
                error.message = %message,
                "http error response"
            );
        } else {
            tracing::warn!(
                http.status = status.as_u16(),
                error.code = code,
                error.message = %message,
                "http error response"
            );
        }

        let body = Json(ErrorBody {
            error: ErrorDetails { code, message },
        });
        (status, body).into_response()
    }
}

/// Default unmatched-route handler for Nidus HTTP applications.
///
/// Install this with [`axum::Router::fallback`] when missing routes should
/// produce the same Nidus JSON error shape as handler-created 404 responses.
/// [`crate::middleware::ApiDefaults::production`] installs it by default.
pub async fn not_found_fallback() -> HttpError {
    HttpError::not_found("route not found")
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

/// Tower layer that converts error responses into a production error envelope.
///
/// Non-error responses pass through unchanged. `4xx` and `5xx` responses are
/// converted to:
///
/// ```json
/// {
///   "error": {
///     "statusCode": 400,
///     "code": "bad_request",
///     "message": "invalid input",
///     "details": null,
///     "timestamp": "2026-01-01T00:00:00Z",
///     "path": "/users",
///     "requestId": "..."
///   }
/// }
/// ```
///
/// Legacy/simple Nidus bodies shaped like
/// `{ "error": { "code": "...", "message": "...", ... } }` are parsed and
/// wrapped. Extra fields under `error` are preserved as `details` for non-5xx
/// responses. For all 5xx responses, the client-facing message is masked to
/// `"internal server error"` and `details` is set to `null`.
/// Error bodies larger than 64 KiB are not parsed as legacy JSON; oversized
/// bodies are replaced with the status-derived envelope to avoid unbounded
/// buffering.
///
/// When [`crate::context::RequestContext`] is present, its request ID is copied
/// into `error.requestId`; otherwise that field is an empty string. Install the
/// validated request ID and request context layers, or use
/// [`crate::middleware::ApiDefaults::production`], when clients need stable
/// request IDs in error responses.
#[derive(Clone, Copy, Debug, Default)]
pub struct ErrorEnvelopeLayer;

impl ErrorEnvelopeLayer {
    /// Creates an error envelope layer.
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ErrorEnvelopeLayer {
    type Service = ErrorEnvelopeService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ErrorEnvelopeService { inner }
    }
}

/// Service produced by [`ErrorEnvelopeLayer`].
#[derive(Clone, Debug)]
pub struct ErrorEnvelopeService<S> {
    inner: S,
}

impl<S> Service<Request> for ErrorEnvelopeService<S>
where
    S: Service<Request, Response = axum::response::Response> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = axum::response::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        // URI and RequestContext clones retain shared backing storage. Defer
        // allocating owned strings until the response actually needs an error
        // envelope; successful responses are the common path.
        let uri = request.uri().clone();
        let request_context = request.extensions().get::<RequestContext>().cloned();
        let future = self.inner.call(request);

        Box::pin(async move {
            let response = future.await?;
            if !response.status().is_client_error() && !response.status().is_server_error() {
                return Ok(response);
            }
            let request_id = request_context
                .as_ref()
                .map(|context| context.request_id().to_owned());
            let path = uri.path().to_owned();
            Ok(envelope_response(response, request_id, path).await)
        })
    }
}

async fn envelope_response(
    response: axum::response::Response,
    request_id: Option<String>,
    path: String,
) -> axum::response::Response {
    let (mut parts, body) = response.into_parts();
    let status = parts.status;
    let extracted = read_legacy_error_body(body).await;
    let mut code = extracted
        .as_ref()
        .map(|body| body.error.code.clone())
        .unwrap_or_else(|| default_code(status).to_owned());
    let mut message = extracted
        .as_ref()
        .map(|body| body.error.message.clone())
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("error").to_owned());
    let mut details = extracted
        .map(|body| {
            if body.error.details.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::Object(body.error.details)
            }
        })
        .unwrap_or(serde_json::Value::Null);
    if status.is_server_error() {
        tracing::error!(
            http.status = status.as_u16(),
            error.code = %code,
            request.id = request_id.as_deref().unwrap_or(""),
            http.path = %path,
            "http error envelope"
        );
        // ERR-1: do not leak internal error taxonomy to clients on a 5xx. The
        // original code is retained in the structured log above for debugging.
        message = "internal server error".to_owned();
        details = serde_json::Value::Null;
        code = default_code(status).to_owned();
    }

    let envelope = ProductionErrorBody {
        error: ProductionErrorDetails {
            status_code: status.as_u16(),
            code,
            message,
            details,
            timestamp: timestamp_now(),
            path,
            request_id: request_id.unwrap_or_default(),
        },
    };
    let body = serde_json::to_vec(&envelope).expect("error envelope should serialize");
    // The inner representation has been replaced. Preserve unrelated response
    // metadata (for example, cookies and rate-limit headers), but never forward
    // representation headers that describe bytes which are no longer present.
    parts.headers.remove(http::header::CONTENT_LENGTH);
    parts.headers.remove(http::header::CONTENT_ENCODING);
    parts.headers.remove(http::header::CONTENT_RANGE);
    parts.headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    axum::response::Response::from_parts(parts, Body::from(body))
}

const MAX_ERROR_ENVELOPE_BODY_BYTES: usize = 64 * 1024;

async fn read_legacy_error_body(body: Body) -> Option<LegacyErrorBody> {
    let bytes = to_bytes(body, MAX_ERROR_ENVELOPE_BODY_BYTES).await.ok()?;
    serde_json::from_slice::<LegacyErrorBody>(&bytes).ok()
}

/// Returns the current UTC timestamp formatted as RFC3339.
pub(crate) fn timestamp_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamp should format as RFC3339")
}

fn default_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "conflict",
        StatusCode::UNPROCESSABLE_ENTITY => "unprocessable_entity",
        StatusCode::TOO_MANY_REQUESTS => "too_many_requests",
        status if status.is_server_error() => "internal_server_error",
        _ => "http_error",
    }
}

#[derive(Debug, Deserialize)]
struct LegacyErrorBody {
    error: LegacyErrorDetails,
}

#[derive(Debug, Deserialize)]
struct LegacyErrorDetails {
    code: String,
    message: String,
    #[serde(flatten)]
    details: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ProductionErrorBody {
    error: ProductionErrorDetails,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductionErrorDetails {
    status_code: u16,
    code: String,
    message: String,
    details: serde_json::Value,
    timestamp: String,
    path: String,
    request_id: String,
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

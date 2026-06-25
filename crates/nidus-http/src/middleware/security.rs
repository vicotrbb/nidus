use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{body::Body, extract::Request};
use http::{HeaderValue, Response, StatusCode, header};
use tower::{Layer, Service};
use tower_http::limit::RequestBodyLimitLayer;

/// Creates a layer that applies conservative API security headers.
///
/// Responses receive:
/// - `x-content-type-options: nosniff`
/// - `x-frame-options: DENY`
/// - `referrer-policy: no-referrer`
///
/// Existing values for those headers are replaced.
pub fn security_headers_layer() -> SecurityHeadersLayer {
    SecurityHeadersLayer
}

/// Tower layer that adds conservative API security headers to responses.
///
/// This layer only mutates response headers after the inner service returns. It
/// does not perform authentication, CORS, CSRF, or content-security-policy
/// enforcement.
#[derive(Clone, Copy, Debug, Default)]
pub struct SecurityHeadersLayer;

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersService { inner }
    }
}

/// Service produced by [`SecurityHeadersLayer`].
#[derive(Clone, Debug)]
pub struct SecurityHeadersService<S> {
    inner: S,
}

impl<S> Service<Request> for SecurityHeadersService<S>
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
        let future = self.inner.call(request);
        Box::pin(async move {
            let mut response = future.await?;
            response.headers_mut().insert(
                "x-content-type-options",
                HeaderValue::from_static("nosniff"),
            );
            response
                .headers_mut()
                .insert("x-frame-options", HeaderValue::from_static("DENY"));
            response
                .headers_mut()
                .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
            Ok(response)
        })
    }
}

/// Creates a request body limit layer using the declared `Content-Length`.
///
/// The layer rejects requests when `Content-Length` parses as `u64` and is
/// greater than `max_bytes`. The rejection is `413 Payload Too Large` with a
/// plain-text `payload too large` body.
///
/// If `Content-Length` is absent, not UTF-8, or not a valid integer, the layer
/// lets the request through. It does not count streamed bytes as the body is
/// read; pair it with extractor/server limits when you need hard streaming
/// enforcement.
pub fn body_limit_layer(max_bytes: u64) -> BodyLimitLayer {
    BodyLimitLayer {
        max_bytes,
        webhook_boundary: false,
    }
}

/// Creates a streaming request body limit layer.
///
/// Unlike [`body_limit_layer`], this wraps the request body and enforces
/// `max_bytes` as the downstream extractor or handler reads the stream. Requests
/// with an oversized `Content-Length` are rejected before the inner service is
/// called; requests without `Content-Length` fail with `413 Payload Too Large`
/// when the body is read past the configured limit.
///
/// Use this when you need a hard read-time cap across streaming bodies. Keep
/// [`body_limit_layer`] when you only want the lightweight declared
/// `Content-Length` boundary used by [`crate::middleware::ApiDefaults`].
pub fn streaming_body_limit_layer(max_bytes: usize) -> RequestBodyLimitLayer {
    RequestBodyLimitLayer::new(max_bytes)
}

/// Creates a request body limit layer for webhook/raw-body routes.
///
/// This has the same declared `Content-Length` behavior as
/// [`body_limit_layer`], but `413` responses include
/// `x-nidus-body-limit: webhook-raw-body`. Use it at raw-body/webhook
/// boundaries where callers or tests need to distinguish this limit from a
/// generic API body limit.
pub fn webhook_body_limit_layer(max_bytes: u64) -> BodyLimitLayer {
    BodyLimitLayer {
        max_bytes,
        webhook_boundary: true,
    }
}

/// Tower layer that rejects requests with a declared oversized body.
///
/// Enforcement is header-based: only a parseable `Content-Length` value above
/// `max_bytes` is rejected. Missing or invalid `Content-Length` values are
/// passed to the inner service unchanged.
#[derive(Clone, Copy, Debug)]
pub struct BodyLimitLayer {
    max_bytes: u64,
    webhook_boundary: bool,
}

impl<S> Layer<S> for BodyLimitLayer {
    type Service = BodyLimitService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BodyLimitService {
            inner,
            max_bytes: self.max_bytes,
            webhook_boundary: self.webhook_boundary,
        }
    }
}

/// Service produced by [`BodyLimitLayer`].
#[derive(Clone, Debug)]
pub struct BodyLimitService<S> {
    inner: S,
    max_bytes: u64,
    webhook_boundary: bool,
}

impl<S> Service<Request> for BodyLimitService<S>
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
        let too_large = request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|length| length > self.max_bytes);
        if too_large {
            let webhook_boundary = self.webhook_boundary;
            return Box::pin(async move { Ok(body_too_large_response(webhook_boundary)) });
        }

        let future = self.inner.call(request);
        Box::pin(future)
    }
}

fn body_too_large_response(webhook_boundary: bool) -> Response<Body> {
    let mut response = Response::new(Body::from("payload too large"));
    *response.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
    if webhook_boundary {
        response.headers_mut().insert(
            "x-nidus-body-limit",
            HeaderValue::from_static("webhook-raw-body"),
        );
    }
    response
}

/// Creates a timeout layer that maps elapsed inner work to `408 Request Timeout`.
///
/// If the inner service completes before `timeout`, its response is returned
/// unchanged. If the timeout elapses first, the response is `408 Request
/// Timeout` with a plain-text `request timed out` body.
pub fn timeout_response_layer(timeout: Duration) -> TimeoutResponseLayer {
    TimeoutResponseLayer { timeout }
}

/// Tower layer that maps elapsed inner work to an HTTP timeout response.
///
/// This is an HTTP response-mapping layer, not Tower's error-returning timeout
/// layer. It keeps the service error type unchanged and turns elapsed requests
/// into a concrete `408` response.
#[derive(Clone, Copy, Debug)]
pub struct TimeoutResponseLayer {
    timeout: Duration,
}

impl<S> Layer<S> for TimeoutResponseLayer {
    type Service = TimeoutResponseService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimeoutResponseService {
            inner,
            timeout: self.timeout,
        }
    }
}

/// Service produced by [`TimeoutResponseLayer`].
#[derive(Clone, Debug)]
pub struct TimeoutResponseService<S> {
    inner: S,
    timeout: Duration,
}

impl<S> Service<Request> for TimeoutResponseService<S>
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
        let timeout_duration = self.timeout;
        let future = self.inner.call(request);
        Box::pin(async move {
            match tokio::time::timeout(timeout_duration, future).await {
                Ok(response) => response,
                Err(_) => Ok(timeout_response()),
            }
        })
    }
}

fn timeout_response() -> Response<Body> {
    let mut response = Response::new(Body::from("request timed out"));
    *response.status_mut() = StatusCode::REQUEST_TIMEOUT;
    response
}

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use axum::{body::Body, extract::Request};
use http::{HeaderValue, Response, StatusCode, header};
use tower::{Layer, Service};

/// Creates a layer that applies conservative API security headers.
pub fn security_headers_layer() -> SecurityHeadersLayer {
    SecurityHeadersLayer
}

/// Tower layer that adds conservative API security headers to responses.
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

/// Creates a request body limit layer using the `Content-Length` boundary.
pub fn body_limit_layer(max_bytes: u64) -> BodyLimitLayer {
    BodyLimitLayer {
        max_bytes,
        webhook_boundary: false,
    }
}

/// Creates a request body limit layer for webhook/raw-body routes.
pub fn webhook_body_limit_layer(max_bytes: u64) -> BodyLimitLayer {
    BodyLimitLayer {
        max_bytes,
        webhook_boundary: true,
    }
}

/// Tower layer that rejects requests with a declared oversized body.
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

/// Creates a timeout layer that maps elapsed work to `408 Request Timeout`.
pub fn timeout_response_layer(timeout: Duration) -> TimeoutResponseLayer {
    TimeoutResponseLayer { timeout }
}

/// Tower layer that maps elapsed inner work to an HTTP timeout response.
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

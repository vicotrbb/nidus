use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{body::Body, response::IntoResponse};
use http::{HeaderValue, Request, Response, StatusCode, header::HeaderName};
use serde::Serialize;
use tower::{Layer, Service};
use uuid::{Uuid, Version};

use crate::context::RequestContext;

/// Tower layer that adds an `x-request-id` response header when absent.
///
/// Incoming request IDs are propagated to the response unless the inner service
/// already set a response ID. Requests without an ID receive a generated one.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// Service produced by [`RequestIdLayer`].
#[derive(Clone, Debug)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S, RequestBody, ResponseBody> Service<Request<RequestBody>> for RequestIdService<S>
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    RequestBody: Send + 'static,
    ResponseBody: Send + 'static,
{
    type Response = Response<ResponseBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<RequestBody>) -> Self::Future {
        let request_id = request.headers().get(request_id_header()).cloned();
        let future = self.inner.call(request);
        Box::pin(async move {
            let mut response = future.await?;
            response
                .headers_mut()
                .entry(request_id_header())
                .or_insert_with(|| request_id.unwrap_or_else(new_request_id));
            Ok(response)
        })
    }
}

fn request_id_header() -> HeaderName {
    HeaderName::from_static("x-request-id")
}

fn new_request_id() -> HeaderValue {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    HeaderValue::from_str(&format!("nidus-{nanos}"))
        .expect("generated request id contains only valid header characters")
}

/// Request ID validation behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestIdMode {
    /// Accept any non-empty incoming request ID.
    Permissive,
    /// Reject malformed incoming request IDs.
    Strict,
}

/// Compatibility alias for naming request ID validation policy.
pub type RequestIdPolicy = RequestIdMode;

/// Typed configuration for validated request ID propagation.
#[derive(Clone)]
pub struct RequestIdConfig {
    header_name: HeaderName,
    mode: RequestIdMode,
    generator: Arc<dyn Fn() -> String + Send + Sync>,
}

impl std::fmt::Debug for RequestIdConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RequestIdConfig")
            .field("header_name", &self.header_name)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl RequestIdConfig {
    /// Creates a production request ID policy with UUID v4 generation and strict validation.
    pub fn production() -> Self {
        Self {
            header_name: HeaderName::from_static("x-request-id"),
            mode: RequestIdMode::Strict,
            generator: Arc::new(|| Uuid::new_v4().to_string()),
        }
    }

    /// Creates a development request ID policy that accepts existing IDs permissively.
    pub fn development() -> Self {
        Self::production().mode(RequestIdMode::Permissive)
    }

    /// Sets the request ID header name.
    pub fn header_name(mut self, header_name: HeaderName) -> Self {
        self.header_name = header_name;
        self
    }

    /// Sets request ID validation behavior.
    pub fn mode(mut self, mode: RequestIdMode) -> Self {
        self.mode = mode;
        self
    }

    /// Replaces the request ID generator.
    pub fn generator(mut self, generator: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.generator = Arc::new(generator);
        self
    }

    /// Returns the configured request ID header name.
    pub fn header(&self) -> &HeaderName {
        &self.header_name
    }

    /// Returns the configured validation mode.
    pub const fn validation_mode(&self) -> RequestIdMode {
        self.mode
    }

    fn generate(&self) -> String {
        (self.generator)()
    }
}

impl Default for RequestIdConfig {
    fn default() -> Self {
        Self::production()
    }
}

/// Creates a validated request ID layer.
pub fn validated_request_id_layer(config: RequestIdConfig) -> ValidatedRequestIdLayer {
    ValidatedRequestIdLayer::new(config)
}

/// Tower layer that validates, generates, stores, and propagates request IDs.
#[derive(Clone, Debug)]
pub struct ValidatedRequestIdLayer {
    config: RequestIdConfig,
}

impl ValidatedRequestIdLayer {
    /// Creates a validated request ID layer from typed config.
    pub fn new(config: RequestIdConfig) -> Self {
        Self { config }
    }
}

impl<S> Layer<S> for ValidatedRequestIdLayer {
    type Service = ValidatedRequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ValidatedRequestIdService {
            inner,
            config: self.config.clone(),
        }
    }
}

/// Service produced by [`ValidatedRequestIdLayer`].
#[derive(Clone, Debug)]
pub struct ValidatedRequestIdService<S> {
    inner: S,
    config: RequestIdConfig,
}

impl<S> Service<Request<Body>> for ValidatedRequestIdService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let config = self.config.clone();
        let (mut parts, body) = request.into_parts();
        let incoming = parts
            .headers
            .get(config.header())
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let request_id = match incoming {
            Some(value) if is_valid_request_id(&value) => value,
            Some(_) if config.validation_mode() == RequestIdMode::Strict => {
                let request_id = config.generate();
                let mut response = invalid_request_id_response(&request_id, parts.uri.path());
                response.headers_mut().insert(
                    config.header().clone(),
                    HeaderValue::from_str(&request_id)
                        .expect("generated request id must be a valid header value"),
                );
                return Box::pin(async move { Ok(response) });
            }
            Some(_) | None => config.generate(),
        };

        let header_value =
            HeaderValue::from_str(&request_id).expect("request id must be a valid header value");
        parts
            .headers
            .insert(config.header().clone(), header_value.clone());
        let context = RequestContext::from_parts(&parts, request_id.clone());
        parts.extensions.insert(context);
        let future = self.inner.call(Request::from_parts(parts, body));

        Box::pin(async move {
            let mut response = future.await?;
            response
                .headers_mut()
                .entry(config.header().clone())
                .or_insert(header_value);
            Ok(response)
        })
    }
}

fn is_valid_request_id(value: &str) -> bool {
    Uuid::parse_str(value)
        .ok()
        .and_then(|uuid| uuid.get_version())
        == Some(Version::Random)
}

fn invalid_request_id_response(request_id: &str, path: &str) -> Response<Body> {
    let timestamp = crate::error::timestamp_now();
    (
        StatusCode::BAD_REQUEST,
        axum::Json(RequestIdErrorBody {
            error: RequestIdErrorDetails {
                status_code: StatusCode::BAD_REQUEST.as_u16(),
                code: "invalid_request_id",
                message: "invalid request id",
                details: serde_json::Value::Null,
                timestamp,
                path: path.to_owned(),
                request_id: request_id.to_owned(),
            },
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
struct RequestIdErrorBody {
    error: RequestIdErrorDetails,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestIdErrorDetails {
    status_code: u16,
    code: &'static str,
    message: &'static str,
    details: serde_json::Value,
    timestamp: String,
    path: String,
    request_id: String,
}

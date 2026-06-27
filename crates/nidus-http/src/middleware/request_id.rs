use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::{body::Body, response::IntoResponse};
use http::{HeaderValue, Request, Response, StatusCode, header::HeaderName};
use serde::Serialize;
use tower::{Layer, Service};
use uuid::{Uuid, Version};

use crate::context::RequestContext;

/// Legacy Tower layer that adds an `x-request-id` response header when absent.
///
/// Incoming request IDs are propagated to the response unless the inner service
/// already set a response ID. Requests without an ID receive a generated
/// UUID v4 value.
///
/// This layer does not validate inbound IDs and does not populate
/// [`RequestContext`]. Prefer [`validated_request_id_layer`] for production API
/// defaults, UUID v4 generation, strict/permissive validation, request
/// extension insertion, and consistent error responses.
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
    HeaderValue::from_str(&Uuid::new_v4().to_string())
        .expect("generated request id contains only valid header characters")
}

/// Request ID validation behavior for inbound `x-request-id` values.
///
/// Valid inbound IDs must parse as UUID v4 values. Invalid header syntax,
/// non-UUID strings, and UUIDs from other versions are treated as malformed.
/// Missing IDs are never rejected; the configured generator is used instead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestIdMode {
    /// Propagate valid UUID v4 IDs and replace malformed incoming IDs.
    ///
    /// This mode is useful in development and at integration boundaries where
    /// clients may still send legacy IDs. A malformed inbound value is not
    /// exposed to handlers; it is replaced with a generated ID before the
    /// request reaches the inner service.
    Permissive,
    /// Propagate valid UUID v4 IDs and reject malformed incoming IDs.
    ///
    /// A malformed inbound value returns `400 Bad Request` with an
    /// `invalid_request_id` JSON error body. The rejection response still
    /// receives a generated request ID in the configured response header.
    Strict,
}

/// Compatibility alias for naming request ID validation policy.
pub type RequestIdPolicy = RequestIdMode;

/// Typed configuration for validated request ID propagation.
///
/// The default production config uses the `x-request-id` header, strict inbound
/// validation, and UUID v4 generation. Custom generators are accepted, but their
/// output must be a valid HTTP header value because generated IDs are inserted
/// into request headers, request extensions, and response headers. If a custom
/// generator returns an invalid header value, the middleware returns a stable
/// `500 Internal Server Error` response with code
/// `invalid_generated_request_id` instead of panicking or calling the inner
/// service.
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
    /// Creates a production request ID policy.
    ///
    /// Defaults:
    /// - header: `x-request-id`
    /// - inbound validation: [`RequestIdMode::Strict`]
    /// - generated IDs: UUID v4 strings
    ///
    /// In strict mode, a present but malformed inbound ID is rejected with
    /// `400 Bad Request`. Missing IDs are generated and accepted.
    pub fn production() -> Self {
        Self {
            header_name: HeaderName::from_static("x-request-id"),
            mode: RequestIdMode::Strict,
            generator: Arc::new(|| Uuid::new_v4().to_string()),
        }
    }

    /// Creates a development request ID policy.
    ///
    /// Development uses the same `x-request-id` header and UUID v4 generator as
    /// production, but switches to [`RequestIdMode::Permissive`]. Valid inbound
    /// UUID v4 IDs are propagated; malformed inbound IDs are replaced with
    /// generated UUID v4 IDs instead of returning `400`.
    pub fn development() -> Self {
        Self::production().mode(RequestIdMode::Permissive)
    }

    /// Sets the request ID header name.
    pub fn header_name(mut self, header_name: HeaderName) -> Self {
        self.header_name = header_name;
        self
    }

    /// Sets request ID validation behavior for present inbound IDs.
    pub fn mode(mut self, mode: RequestIdMode) -> Self {
        self.mode = mode;
        self
    }

    /// Replaces the request ID generator.
    ///
    /// [`RequestIdConfig::production`] and [`RequestIdConfig::development`] use
    /// UUID v4 strings. If you provide a custom generator, keep it deterministic
    /// enough for your tests and ensure it returns values that can be stored in
    /// an HTTP header. Invalid generated header values return a structured
    /// framework error response before the request reaches the inner service.
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
///
/// The layer validates or generates a request ID before the inner service runs,
/// inserts that value into the configured request header, stores a
/// [`RequestContext`] in request extensions, and mirrors the same header onto
/// the response when the inner service has not already set it.
///
/// ```ignore
/// use axum::{Router, routing::get};
/// use nidus_http::middleware::{
///     RequestIdConfig, RequestIdMode, validated_request_id_layer,
/// };
///
/// let app = Router::new()
///     .route("/users/:id", get(handler))
///     .layer(validated_request_id_layer(
///         RequestIdConfig::production().mode(RequestIdMode::Strict),
///     ));
/// ```
pub fn validated_request_id_layer(config: RequestIdConfig) -> ValidatedRequestIdLayer {
    ValidatedRequestIdLayer::new(config)
}

/// Tower layer that validates, generates, stores, and propagates request IDs.
///
/// Valid inbound request IDs are UUID v4 strings. With
/// [`RequestIdMode::Strict`], malformed inbound IDs receive `400 Bad Request`.
/// With [`RequestIdMode::Permissive`], malformed inbound IDs are replaced with a
/// generated ID. Generated IDs are UUID v4 by default.
///
/// On accepted requests, the final ID is inserted into the configured request
/// header, added to request extensions through [`RequestContext`], and copied to
/// the response header if the inner service did not set one. Use
/// [`crate::middleware::request_context_layer`] after this layer when you want
/// the context enriched with route and correlation fields before handlers run.
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
                let (request_id, header_value) = match generated_request_id_header(&config) {
                    Some(generated) => generated,
                    None => {
                        return Box::pin(async move {
                            Ok(invalid_generated_request_id_response(parts.uri.path()))
                        });
                    }
                };
                let mut response = invalid_request_id_response(&request_id, parts.uri.path());
                response
                    .headers_mut()
                    .insert(config.header().clone(), header_value);
                return Box::pin(async move { Ok(response) });
            }
            Some(_) | None => {
                let (request_id, header_value) = match generated_request_id_header(&config) {
                    Some(generated) => generated,
                    None => {
                        return Box::pin(async move {
                            Ok(invalid_generated_request_id_response(parts.uri.path()))
                        });
                    }
                };
                parts
                    .headers
                    .insert(config.header().clone(), header_value.clone());
                let context = RequestContext::from_parts(&parts, request_id.clone());
                parts.extensions.insert(context);
                let future = self.inner.call(Request::from_parts(parts, body));

                return Box::pin(async move {
                    let mut response = future.await?;
                    response
                        .headers_mut()
                        .entry(config.header().clone())
                        .or_insert(header_value);
                    Ok(response)
                });
            }
        };

        let header_value = HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| unreachable!("accepted inbound request id came from a header"));
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

fn generated_request_id_header(config: &RequestIdConfig) -> Option<(String, HeaderValue)> {
    let request_id = config.generate();
    let header_value = HeaderValue::from_str(&request_id).ok()?;
    Some((request_id, header_value))
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
                request_id: Some(request_id.to_owned()),
            },
        }),
    )
        .into_response()
}

fn invalid_generated_request_id_response(path: &str) -> Response<Body> {
    let timestamp = crate::error::timestamp_now();
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(RequestIdErrorBody {
            error: RequestIdErrorDetails {
                status_code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                code: "invalid_generated_request_id",
                message: "generated request id was not a valid HTTP header value",
                details: serde_json::Value::Null,
                timestamp,
                path: path.to_owned(),
                request_id: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

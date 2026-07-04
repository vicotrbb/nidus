use std::task::{Context, Poll};

use axum::extract::Request;
use tower::{Layer, Service};

use crate::context::{RequestContext, header_to_string};

/// Creates a Tower layer that enriches [`RequestContext`] request extensions.
///
/// Use this with [`crate::middleware::validated_request_id_layer`] so handlers
/// can extract [`RequestContext`]. The request ID layer chooses and stores the
/// final ID; this layer rebuilds the context from request parts so correlation,
/// trace, route, and client-kind fields reflect the current request boundary.
/// [`crate::middleware::ApiDefaults::production`] installs both layers.
///
/// If no prior context or `x-request-id` header exists, the context uses
/// `"unknown"` as the request ID. Prefer validated request IDs for production
/// APIs.
pub fn request_context_layer() -> RequestContextLayer {
    RequestContextLayer
}

/// Tower layer that inserts request/correlation context into request extensions.
///
/// The inserted context reads:
/// - `x-request-id` from the existing [`RequestContext`] or request header
/// - `x-correlation-id`, falling back to the request ID
/// - `traceparent` trace ID
/// - `x-api-key` / `Authorization` for client classification
/// - Axum [`axum::extract::MatchedPath`] when available at this layer
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestContextLayer;

impl<S> Layer<S> for RequestContextLayer {
    type Service = RequestContextService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestContextService { inner }
    }
}

/// Service produced by [`RequestContextLayer`].
#[derive(Clone, Debug)]
pub struct RequestContextService<S> {
    inner: S,
}

impl<S> Service<Request> for RequestContextService<S>
where
    S: Service<Request> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    // All context work happens before the inner call, so the inner future can
    // be returned directly without a per-request box.
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let (mut parts, body) = request.into_parts();
        let request_id = parts
            .extensions
            .remove::<RequestContext>()
            .map(RequestContext::into_request_id)
            .or_else(|| header_to_string(&parts.headers, "x-request-id"))
            .unwrap_or_else(|| "unknown".to_owned());
        let context = RequestContext::from_parts(&parts, request_id);
        parts.extensions.insert(context);
        self.inner.call(Request::from_parts(parts, body))
    }
}

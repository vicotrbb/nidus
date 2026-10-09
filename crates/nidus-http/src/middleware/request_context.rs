use axum::extract::Request;
use tower::{
    Layer,
    util::{MapRequest, MapRequestLayer},
};

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
/// - validated `traceparent` trace and parent span IDs
/// - `x-api-key` / `Authorization` for client classification
/// - Axum [`axum::extract::MatchedPath`] when available at this layer
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestContextLayer;

impl<S> Layer<S> for RequestContextLayer {
    type Service = RequestContextService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MapRequestLayer::new(enrich_request_context as fn(Request) -> Request).layer(inner)
    }
}

/// Service produced by [`RequestContextLayer`].
pub type RequestContextService<S> = MapRequest<S, fn(Request) -> Request>;

fn enrich_request_context(request: Request) -> Request {
    let (mut parts, body) = request.into_parts();
    let request_id = parts
        .extensions
        .remove::<RequestContext>()
        .map(RequestContext::into_request_id)
        .or_else(|| header_to_string(&parts.headers, "x-request-id"))
        .unwrap_or_else(|| "unknown".to_owned());
    let context = RequestContext::from_parts(&parts, request_id);
    parts.extensions.insert(context);
    Request::from_parts(parts, body)
}

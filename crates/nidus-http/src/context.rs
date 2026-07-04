//! Request context primitives shared by middleware, handlers, and observers.

use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
};

use axum::extract::FromRequestParts;
use http::{HeaderMap, Method, request::Parts};
use serde::Serialize;

/// Client classification inferred from request boundary headers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    /// Request uses an API key style credential.
    ApiKey,
    /// Request carries a bearer token or other authorization header.
    Authenticated,
    /// Request has no recognized application credential.
    Anonymous,
}

impl ClientKind {
    /// Returns the stable string label for this client kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Authenticated => "authenticated",
            Self::Anonymous => "anonymous",
        }
    }
}

/// Request/correlation context attached to request extensions.
///
/// `RequestContext` is available to handlers when the router uses
/// [`crate::middleware::validated_request_id_layer`] plus
/// [`crate::middleware::request_context_layer`], or when it is wrapped by
/// [`crate::middleware::ApiDefaults::production`]. Extracting it without those
/// extensions rejects the request with `500 Internal Server Error`.
///
/// Fields are inferred from request headers and Axum extensions:
/// - `request_id`: the final validated/generated `x-request-id`
/// - `correlation_id`: `x-correlation-id`, falling back to the request ID
/// - `trace_id`: the trace-id segment from `traceparent`
/// - `client_kind`: `x-api-key` means API key, otherwise `Authorization` means
///   authenticated, otherwise anonymous
/// - `route`: Axum's [`axum::extract::MatchedPath`] when it is available at the
///   point the context layer runs
///
/// ```
/// use nidus_http::{Json, context::RequestContext};
///
/// async fn handler(context: RequestContext) -> Json<serde_json::Value> {
///     Json(serde_json::json!({
///         "requestId": context.request_id(),
///         "correlationId": context.correlation_id(),
///     }))
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestContext {
    request_id: String,
    correlation_id: Option<String>,
    method: Method,
    route: Option<String>,
    path: String,
    trace_id: Option<String>,
    span_id: Option<String>,
    client_kind: ClientKind,
    user_id: Option<String>,
    tenant_id: Option<String>,
    session_id: Option<String>,
}

impl RequestContext {
    /// Creates a context for the current request boundary.
    ///
    /// This constructor is useful in tests or custom middleware. It does not
    /// inspect headers, so optional correlation, trace, route, and client fields
    /// remain empty/default until set explicitly or built via [`Self::from_parts`].
    pub fn new(request_id: impl Into<String>, method: Method, path: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            correlation_id: None,
            method,
            route: None,
            path: path.into(),
            trace_id: None,
            span_id: None,
            client_kind: ClientKind::Anonymous,
            user_id: None,
            tenant_id: None,
            session_id: None,
        }
    }

    /// Creates a context from request parts.
    ///
    /// This reads `x-correlation-id`, `traceparent`, `x-api-key`,
    /// `Authorization`, and [`axum::extract::MatchedPath`] from the request
    /// boundary. The supplied `request_id` is expected to be the final ID chosen
    /// by request ID middleware.
    pub fn from_parts(parts: &Parts, request_id: impl Into<String>) -> Self {
        let request_id = request_id.into();
        let correlation_id = header_to_string(&parts.headers, "x-correlation-id")
            .or_else(|| (!request_id.is_empty()).then(|| request_id.clone()));
        let mut context = Self::new(request_id, parts.method.clone(), parts.uri.path());
        context.correlation_id = correlation_id;
        context.route = parts
            .extensions
            .get::<axum::extract::MatchedPath>()
            .map(|path| path.as_str().to_owned());
        context.client_kind = infer_client_kind(&parts.headers);
        context.trace_id = header_to_string(&parts.headers, "traceparent")
            .and_then(|value| value.split('-').nth(1).map(str::to_owned));
        context
    }

    /// Returns the final request id.
    ///
    /// With [`crate::middleware::validated_request_id_layer`], this is either a
    /// valid inbound UUID v4 or a generated ID.
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(crate) fn into_request_id(self) -> String {
        self.request_id
    }

    /// Returns the correlation id when available.
    ///
    /// [`Self::from_parts`] prefers `x-correlation-id` and falls back to the
    /// request ID when no correlation header is present.
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    /// Returns the request method.
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the stable matched route pattern when available.
    ///
    /// This depends on Axum's [`axum::extract::MatchedPath`] extension being
    /// present before the context is built. Layer placement can affect whether
    /// this is available for a given router shape.
    pub fn route(&self) -> Option<&str> {
        self.route.as_deref()
    }

    /// Returns the raw request path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the trace id when available.
    ///
    /// The value is extracted from the second segment of the W3C `traceparent`
    /// header. Use [`crate::otel::extract_trace_context`] when the `otel`
    /// feature is enabled and you need full trace/span validation.
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    /// Returns the span id when available.
    pub fn span_id(&self) -> Option<&str> {
        self.span_id.as_deref()
    }

    /// Returns the inferred client kind.
    ///
    /// `x-api-key` takes precedence over `Authorization`; otherwise the request
    /// is classified as anonymous.
    pub const fn client_kind(&self) -> ClientKind {
        self.client_kind
    }

    /// Returns the optional application user id.
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    /// Returns the optional application tenant id.
    pub fn tenant_id(&self) -> Option<&str> {
        self.tenant_id.as_deref()
    }

    /// Returns the optional application session id.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Sets the stable matched route pattern.
    pub fn with_route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    /// Sets an application user id.
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Sets an application tenant id.
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Sets an application session id.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = axum::http::StatusCode;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let context = parts.extensions.get::<Self>().cloned();
        async move { context.ok_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR) }
    }
}

/// Request identity used by rate limiters and observers.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RequestIdentity(String);

impl RequestIdentity {
    /// Creates a request identity from a stable label.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identity label.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Extracts a rate-limit identity from request parts.
pub trait IdentityExtractor: Clone + Send + Sync + 'static {
    /// Returns the identity for this request.
    fn extract(&self, parts: &Parts) -> Option<RequestIdentity>;
}

impl<F> IdentityExtractor for F
where
    F: Fn(&Parts) -> Option<RequestIdentity> + Clone + Send + Sync + 'static,
{
    fn extract(&self, parts: &Parts) -> Option<RequestIdentity> {
        self(parts)
    }
}

/// Builds an identity extractor that prefers user/tenant/API key context fields.
pub fn context_identity() -> impl IdentityExtractor {
    |parts: &Parts| {
        if let Some(context) = parts.extensions.get::<RequestContext>()
            && let Some(value) = context.user_id().or_else(|| context.tenant_id())
        {
            return Some(RequestIdentity::new(value.to_owned()));
        }
        header_to_string(&parts.headers, "x-api-key").map(RequestIdentity::new)
    }
}

/// Builds an identity extractor from API key headers.
pub fn api_key_identity() -> impl IdentityExtractor {
    |parts: &Parts| header_to_string(&parts.headers, "x-api-key").map(RequestIdentity::new)
}

/// Builds an identity extractor from the connected client IP address.
///
/// This extractor uses Axum's [`axum::extract::ConnectInfo<SocketAddr>`]
/// extension and ignores `X-Forwarded-For`. Nidus serving helpers populate
/// `ConnectInfo` on the normal `listen`/`serve` path. If a router is exercised
/// without peer information, the identity falls back to `"anonymous"`.
pub fn client_ip_identity() -> impl IdentityExtractor {
    |parts: &Parts| {
        peer_ip(parts)
            .map(|ip| RequestIdentity::new(ip.to_string()))
            .or_else(|| Some(RequestIdentity::new("anonymous")))
    }
}

/// Builds an identity extractor that trusts `X-Forwarded-For` only from known proxies.
///
/// Use this when Nidus runs behind a reverse proxy that rewrites or appends
/// `X-Forwarded-For` and the direct peer address is one of the configured
/// trusted proxy IPs. Requests from untrusted peers ignore `X-Forwarded-For`
/// and use the direct peer IP. Requests without peer information fall back to
/// `"anonymous"`.
pub fn trusted_proxy_client_ip_identity(
    trusted_proxies: impl IntoIterator<Item = IpAddr>,
) -> impl IdentityExtractor {
    let trusted_proxies = trusted_proxies.into_iter().collect::<Vec<_>>();
    move |parts: &Parts| {
        peer_ip(parts)
            .map(|peer| {
                if trusted_proxies.contains(&peer)
                    && let Some(forwarded_ip) = forwarded_for_ip(&parts.headers)
                {
                    RequestIdentity::new(forwarded_ip.to_string())
                } else {
                    RequestIdentity::new(peer.to_string())
                }
            })
            .or_else(|| Some(RequestIdentity::new("anonymous")))
    }
}

fn peer_ip(parts: &Parts) -> Option<IpAddr> {
    parts
        .extensions
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0.ip())
}

fn forwarded_for_ip(headers: &HeaderMap) -> Option<IpAddr> {
    header_to_string(headers, "x-forwarded-for").and_then(|value| {
        value
            .split(',')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok())
    })
}

pub(crate) fn header_to_string(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn infer_client_kind(headers: &HeaderMap) -> ClientKind {
    if headers.contains_key("x-api-key") {
        ClientKind::ApiKey
    } else if headers.contains_key(http::header::AUTHORIZATION) {
        ClientKind::Authenticated
    } else {
        ClientKind::Anonymous
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;

    #[test]
    fn request_context_can_consume_request_id() {
        let context = RequestContext::new("req-123", Method::GET, "/users");

        assert_eq!(context.into_request_id(), "req-123");
    }

    #[test]
    fn client_ip_identity_ignores_forwarded_headers_without_peer_info() {
        let parts = request_parts(None, Some("203.0.113.10"));
        let identity = client_ip_identity().extract(&parts).unwrap();

        assert_eq!(identity.as_str(), "anonymous");
    }

    #[test]
    fn trusted_proxy_client_ip_identity_uses_forwarded_header_from_trusted_peer() {
        let parts = request_parts(Some("127.0.0.1:5000"), Some("203.0.113.10, 10.0.0.5"));
        let trusted_proxy = "127.0.0.1".parse::<IpAddr>().unwrap();
        let identity = trusted_proxy_client_ip_identity([trusted_proxy])
            .extract(&parts)
            .unwrap();

        assert_eq!(identity.as_str(), "203.0.113.10");
    }

    #[test]
    fn trusted_proxy_client_ip_identity_ignores_forwarded_header_from_untrusted_peer() {
        let parts = request_parts(Some("127.0.0.1:5000"), Some("203.0.113.10"));
        let trusted_proxy = "10.0.0.1".parse::<IpAddr>().unwrap();
        let identity = trusted_proxy_client_ip_identity([trusted_proxy])
            .extract(&parts)
            .unwrap();

        assert_eq!(identity.as_str(), "127.0.0.1");
    }

    fn request_parts(peer: Option<&str>, forwarded_for: Option<&str>) -> Parts {
        let mut builder = Request::builder().uri("/");
        if let Some(forwarded_for) = forwarded_for {
            builder = builder.header("x-forwarded-for", forwarded_for);
        }
        let (mut parts, ()) = builder.body(()).unwrap().into_parts();
        if let Some(peer) = peer {
            parts.extensions.insert(axum::extract::ConnectInfo(
                peer.parse::<SocketAddr>().unwrap(),
            ));
        }
        parts
    }
}

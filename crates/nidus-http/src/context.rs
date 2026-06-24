//! Request context primitives shared by middleware, handlers, and observers.

use std::{future::Future, net::SocketAddr};

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
    pub fn from_parts(parts: &Parts, request_id: impl Into<String>) -> Self {
        let request_id = request_id.into();
        let mut context = Self::new(request_id.clone(), parts.method.clone(), parts.uri.path());
        context.correlation_id = header_to_string(&parts.headers, "x-correlation-id")
            .or_else(|| Some(request_id).filter(|value| !value.is_empty()));
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
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Returns the correlation id when available.
    pub fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    /// Returns the request method.
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the stable matched route pattern when available.
    pub fn route(&self) -> Option<&str> {
        self.route.as_deref()
    }

    /// Returns the raw request path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the trace id when available.
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    /// Returns the span id when available.
    pub fn span_id(&self) -> Option<&str> {
        self.span_id.as_deref()
    }

    /// Returns the inferred client kind.
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
pub fn client_ip_identity() -> impl IdentityExtractor {
    |parts: &Parts| {
        parts
            .extensions
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|connect| RequestIdentity::new(connect.0.ip().to_string()))
            .or_else(|| {
                header_to_string(&parts.headers, "x-forwarded-for")
                    .and_then(|value| value.split(',').next().map(str::trim).map(str::to_owned))
                    .filter(|value| !value.is_empty())
                    .map(RequestIdentity::new)
            })
            .or_else(|| Some(RequestIdentity::new("anonymous")))
    }
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

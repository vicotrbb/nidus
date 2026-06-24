//! Request extractors and helpers.

use std::{future::Future, ops::Deref, sync::Arc};

use axum::{Json, extract::FromRequestParts, response::IntoResponse};
use http::{StatusCode, request::Parts};
use nidus_core::{Inject, NidusError, SharedRequestScope};
use serde::Serialize;

/// Axum extractor for a provider resolved from the active Nidus request scope.
///
/// Attach `request_scope_layer(container)` to the router before using this
/// extractor in handlers.
#[derive(Clone, Debug)]
pub struct RequestScoped<T: Send + Sync + 'static>(Inject<T>);

impl<T> RequestScoped<T>
where
    T: Send + Sync + 'static,
{
    /// Creates a request-scoped extractor value from an injected dependency.
    pub fn new(value: Inject<T>) -> Self {
        Self(value)
    }

    /// Returns the underlying injected dependency wrapper.
    pub fn into_inject(self) -> Inject<T> {
        self.0
    }

    /// Returns a cloned shared pointer to the resolved dependency.
    pub fn into_inner(self) -> Arc<T> {
        self.0.into_inner()
    }
}

impl<T> Deref for RequestScoped<T>
where
    T: Send + Sync + 'static,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<S, T> FromRequestParts<S> for RequestScoped<T>
where
    S: Send + Sync,
    T: Send + Sync + 'static,
{
    type Rejection = RequestScopeRejection;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let scope = parts.extensions.get::<SharedRequestScope>().cloned();
        async move {
            let scope = scope.ok_or(RequestScopeRejection::MissingScope)?;
            scope
                .inject::<T>()
                .map(Self::new)
                .map_err(RequestScopeRejection::ResolutionFailed)
        }
    }
}

/// Rejection returned when a request-scoped provider cannot be extracted.
#[derive(Debug, thiserror::Error)]
pub enum RequestScopeRejection {
    /// The request did not contain a Nidus request scope.
    #[error("request scope is not available; attach request_scope_layer to the router")]
    MissingScope,
    /// The request scope failed to resolve the requested provider.
    #[error("request-scoped provider resolution failed: {0}")]
    ResolutionFailed(#[source] NidusError),
}

impl IntoResponse for RequestScopeRejection {
    fn into_response(self) -> axum::response::Response {
        let (code, message) = match self {
            Self::MissingScope => (
                "request_scope_unavailable",
                "request scope is not available; attach request_scope_layer to the router"
                    .to_owned(),
            ),
            Self::ResolutionFailed(error) => (
                "request_scope_resolution_failed",
                format!("request-scoped provider resolution failed: {error}"),
            ),
        };
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                error: ErrorDetails { code, message },
            }),
        )
            .into_response()
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

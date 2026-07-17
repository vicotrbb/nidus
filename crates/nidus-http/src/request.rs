//! Request extractors and helpers.

use std::{future::Future, ops::Deref, sync::Arc};

use axum::{Json, extract::FromRequestParts, response::IntoResponse};
use http::{StatusCode, request::Parts};
use nidus_core::{Inject, NidusError, SharedRequestScope};
use serde::Serialize;

/// Axum extractor for a provider resolved from the active Nidus request scope.
///
/// Attach [`crate::middleware::request_scope_layer`] with the application
/// [`nidus_core::Container`] before using this extractor. The requested type
/// must be registered in the container, commonly with
/// `Container::register_request` or `Container::register_request_scoped`.
///
/// Missing middleware rejects with `500 Internal Server Error` and
/// `request_scope_unavailable`. A provider resolution failure also returns
/// `500`, with `request_scope_resolution_failed`. Resolution details are logged
/// server-side and omitted from the client response.
///
/// `RequestScoped<T>` dereferences to `T` for handler reads. Use
/// [`Self::into_inner`] when you need the shared [`Arc<T>`], or
/// [`Self::into_inject`] when passing the value to APIs that expect Nidus'
/// [`Inject<T>`] wrapper.
///
/// ```
/// use std::sync::Arc;
/// use axum::{Router, routing::get};
/// use nidus_core::Container;
/// use nidus_http::{RequestScoped, middleware::request_scope_layer};
///
/// struct CurrentTenant(String);
///
/// async fn handler(tenant: RequestScoped<CurrentTenant>) -> String {
///     tenant.0.clone()
/// }
///
/// let mut container = Container::new();
/// container.register_request::<CurrentTenant, _>(|_container| {
///     Ok(CurrentTenant("demo".to_owned()))
/// })?;
///
/// let app = Router::new()
///     .route("/tenant", get(handler))
///     .layer(request_scope_layer(Arc::new(container)));
/// # let _: Router = app;
/// # Ok::<(), nidus_core::NidusError>(())
/// ```
#[derive(Clone, Debug)]
pub struct RequestScoped<T: Send + Sync + 'static>(Inject<T>);

impl<T> RequestScoped<T>
where
    T: Send + Sync + 'static,
{
    /// Creates a request-scoped extractor value from an injected dependency.
    ///
    /// Most application code receives this from Axum extraction rather than
    /// constructing it manually.
    pub fn new(value: Inject<T>) -> Self {
        Self(value)
    }

    /// Returns the underlying injected dependency wrapper.
    ///
    /// Use this when downstream Nidus APIs need the injection wrapper rather
    /// than a borrowed `T` or shared [`Arc<T>`].
    pub fn into_inject(self) -> Inject<T> {
        self.0
    }

    /// Returns the shared pointer to the resolved dependency.
    ///
    /// This is useful when spawning work that must own the provider beyond the
    /// handler's borrow.
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
                "request scope is not available; attach request_scope_layer to the router",
            ),
            Self::ResolutionFailed(error) => {
                tracing::error!(error = %error, "request-scoped provider resolution failed");
                (
                    "request_scope_resolution_failed",
                    "request-scoped provider resolution failed",
                )
            }
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
    message: &'static str,
}

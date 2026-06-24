//! Axum router composition.

mod metadata;
mod path;

use axum::{Router, handler::Handler, routing};
use http::Method;

use crate::error::RoutePathError;

pub use metadata::RouteMetadata;
pub(crate) use path::join_paths;
use path::normalize_path;

/// A route declaration that can be mounted by a controller.
pub struct RouteDefinition {
    method: Method,
    path: String,
    route: routing::MethodRouter,
}

impl RouteDefinition {
    /// Creates a GET route.
    pub fn get<H, T>(path: impl Into<String>, handler: H) -> Self
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_get(path, handler).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create a GET route.
    pub fn try_get<H, T>(path: impl Into<String>, handler: H) -> Result<Self, RoutePathError>
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_new(Method::GET, path, routing::get(handler))
    }

    /// Creates a POST route.
    pub fn post<H, T>(path: impl Into<String>, handler: H) -> Self
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_post(path, handler).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create a POST route.
    pub fn try_post<H, T>(path: impl Into<String>, handler: H) -> Result<Self, RoutePathError>
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_new(Method::POST, path, routing::post(handler))
    }

    /// Creates a PUT route.
    pub fn put<H, T>(path: impl Into<String>, handler: H) -> Self
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_put(path, handler).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create a PUT route.
    pub fn try_put<H, T>(path: impl Into<String>, handler: H) -> Result<Self, RoutePathError>
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_new(Method::PUT, path, routing::put(handler))
    }

    /// Creates a PATCH route.
    pub fn patch<H, T>(path: impl Into<String>, handler: H) -> Self
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_patch(path, handler).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create a PATCH route.
    pub fn try_patch<H, T>(path: impl Into<String>, handler: H) -> Result<Self, RoutePathError>
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_new(Method::PATCH, path, routing::patch(handler))
    }

    /// Creates a DELETE route.
    pub fn delete<H, T>(path: impl Into<String>, handler: H) -> Self
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_delete(path, handler).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create a DELETE route.
    pub fn try_delete<H, T>(path: impl Into<String>, handler: H) -> Result<Self, RoutePathError>
    where
        H: Handler<T, ()> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        Self::try_new(Method::DELETE, path, routing::delete(handler))
    }

    /// Returns the route path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the route method.
    pub fn method(&self) -> &Method {
        &self.method
    }

    pub(crate) fn into_router(self, full_path: String) -> Router {
        Router::new().route(&full_path, self.route)
    }

    fn try_new(
        method: Method,
        path: impl Into<String>,
        route: routing::MethodRouter,
    ) -> Result<Self, RoutePathError> {
        Ok(Self {
            method,
            path: normalize_path(path.into())?,
            route,
        })
    }
}

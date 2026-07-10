//! Controller metadata.

use axum::Router;

use crate::error::RoutePathError;
use crate::router::{RouteDefinition, join_normalized_paths, normalize_mount_prefix};

/// Controller route group with a shared path prefix.
pub struct Controller {
    prefix: String,
    routes: Vec<RouteDefinition>,
}

impl Controller {
    /// Creates an empty controller route group.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            routes: Vec::new(),
        }
    }

    /// Tries to create an empty controller route group with a validated prefix.
    pub fn try_new(prefix: impl Into<String>) -> Result<Self, RoutePathError> {
        normalize_mount_prefix(prefix.into()).map(|prefix| Self {
            prefix,
            routes: Vec::new(),
        })
    }

    /// Adds a route to this controller.
    pub fn route(mut self, route: RouteDefinition) -> Self {
        self.routes.push(route);
        self
    }

    /// Builds an Axum router from the controller routes.
    pub fn into_router(self) -> Router {
        self.try_into_router()
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to build an Axum router from the controller routes.
    pub fn try_into_router(self) -> Result<Router, RoutePathError> {
        let prefix = normalize_mount_prefix(&self.prefix)?;
        let mut router = Router::new();
        for route in self.routes {
            // RouteDefinition normalizes its path at construction, so only the
            // controller prefix needs validation here. Avoid re-normalizing
            // both strings for every route in the controller.
            let full_path = join_normalized_paths(&prefix, route.path());
            router = route.mount(router, full_path);
        }
        Ok(router)
    }
}

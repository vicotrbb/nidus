//! Shared controller assembly and application request context.

use crate::{middleware::request_scope_layer, router::RouteMetadata};
use axum::{Extension, Router};
use nidus_core::{Container, ModuleGraph, NidusError, Result};
use std::{any::Any, collections::BTreeSet, sync::Arc};

/// Validates all declared controller paths before any resource initialization.
/// Opaque manual routers and controller factories are checked only when assembled.
pub fn validate_routes(graph: &ModuleGraph) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut declared = Router::new();
    for module in graph.modules() {
        for controller in module.controller_descriptors() {
            for route in controller_routes(controller.route_metadata())? {
                let path = route.try_full_path(controller.prefix()).map_err(|error| {
                    NidusError::ApplicationBuild {
                        message: error.to_string(),
                    }
                })?;
                let key = format!("{} {path}", route.method());
                let method: http::Method =
                    route
                        .method()
                        .parse()
                        .map_err(|error| NidusError::ApplicationBuild {
                            message: format!("invalid route method: {error}"),
                        })?;
                let filter = axum::routing::MethodFilter::try_from(method).map_err(|error| {
                    NidusError::ApplicationBuild {
                        message: error.to_string(),
                    }
                })?;
                declared =
                    assemble(|| Ok(declared.route(&path, axum::routing::on(filter, || async {}))))?;
                if !seen.insert(key.clone()) {
                    return Err(NidusError::ApplicationBuild {
                        message: format!("duplicate route `{key}`"),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Builds module-owned controllers using the application container.
pub fn controller_router(graph: &ModuleGraph, container: &Container) -> Result<Router> {
    let mut router = Router::new();
    for module in graph.modules() {
        for controller in module.controller_descriptors() {
            router = router.merge(
                *controller
                    .build_router(container)?
                    .downcast::<Router>()
                    .map_err(|_| NidusError::ApplicationBuild {
                        message: "controller returned an unexpected router type".to_owned(),
                    })?,
            );
        }
    }
    Ok(router)
}

/// Reads declared HTTP metadata without constructing controllers.
pub fn controller_routes(value: Box<dyn Any + Send + Sync>) -> Result<Vec<RouteMetadata>> {
    value
        .downcast::<Vec<RouteMetadata>>()
        .map(|routes| *routes)
        .map_err(|_| NidusError::ApplicationBuild {
            message: "controller returned unexpected route metadata".to_owned(),
        })
}

/// Installs the same container for handler extraction and request-scope resolution.
/// No request-scope middleware is installed if no request providers are registered.
pub fn application_context(router: Router, container: Arc<Container>) -> Router {
    let router = router.layer(Extension(Arc::clone(&container)));
    if container.requires_request_scope() {
        router.layer(request_scope_layer(container))
    } else {
        router
    }
}

/// Runs synchronous router assembly with an unwind boundary for opaque Axum merges.
/// This turns route/factory panics into build errors so initialized resources can roll back.
pub fn assemble(build: impl FnOnce() -> Result<Router>) -> Result<Router> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)).unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_else(|| "non-string panic payload".to_owned());
        Err(NidusError::ApplicationBuild {
            message: format!("router assembly panicked: {message}"),
        })
    })
}

/// Constructs synchronous controllers on the blocking executor while preserving
/// the plan for rollback on every ordinary controller error or unwind.
pub async fn construct_controllers(
    plan: nidus_core::ApplicationPlan,
) -> Result<(nidus_core::ApplicationPlan, Result<Router>)> {
    tokio::task::spawn_blocking(move || {
        let router = assemble(|| controller_router(plan.graph(), plan.container()));
        (plan, router)
    })
    .await
    .map_err(|error| NidusError::ApplicationBuild {
        message: format!("controller construction task failed: {error}"),
    })
}

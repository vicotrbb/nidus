//! High-level application composition for the facade crate.

use std::collections::BTreeSet;

use nidus_core::{Application, Container, Module, ModuleGraph, Nidus, NidusError, Result};

#[cfg(feature = "http")]
use nidus_http::{
    Router,
    server::{ApplicationHttpExt, HttpApplication},
};
#[cfg(feature = "observability")]
use nidus_observability::{Observability, OperationStatus};
#[cfg(feature = "openapi")]
use nidus_openapi::OpenApiDocument;
#[cfg(feature = "dashboard")]
use nidus_dashboard::{DashboardRouteSnapshot, NidusDashboard};

/// Extension methods for creating composed Nidus applications from root modules.
pub trait NidusApplicationExt {
    /// Creates an application builder driven by the root module graph.
    fn create<M>() -> NidusApplicationBuilder<M>
    where
        M: Module;
}

impl NidusApplicationExt for Nidus {
    fn create<M>() -> NidusApplicationBuilder<M>
    where
        M: Module,
    {
        NidusApplicationBuilder::new()
    }
}

/// Module-driven application builder.
pub struct NidusApplicationBuilder<M>
where
    M: Module,
{
    container: Container,
    openapi: Option<OpenApiOptions>,
    tracing: bool,
    #[cfg(feature = "http")]
    routers: Vec<Router>,
    #[cfg(feature = "observability")]
    observability: Option<Observability>,
    #[cfg(feature = "dashboard")]
    dashboard: Option<NidusDashboard>,
    #[cfg(feature = "dashboard")]
    dashboard_route_snapshots: Vec<DashboardRouteSnapshot>,
    #[cfg(feature = "openapi")]
    schemas: Vec<fn(OpenApiDocument) -> OpenApiDocument>,
    _module: std::marker::PhantomData<M>,
}

impl<M> NidusApplicationBuilder<M>
where
    M: Module,
{
    fn new() -> Self {
        Self {
            container: Container::new(),
            openapi: None,
            tracing: false,
            #[cfg(feature = "http")]
            routers: Vec::new(),
            #[cfg(feature = "observability")]
            observability: None,
            #[cfg(feature = "dashboard")]
            dashboard: None,
            #[cfg(feature = "dashboard")]
            dashboard_route_snapshots: Vec::new(),
            #[cfg(feature = "openapi")]
            schemas: Vec::new(),
            _module: std::marker::PhantomData,
        }
    }

    /// Adds an externally-created singleton value before module providers initialize.
    pub fn with_singleton<T>(mut self, value: T) -> Result<Self>
    where
        T: Send + Sync + 'static,
    {
        self.container.register_singleton(value)?;
        Ok(self)
    }

    /// Enables generated OpenAPI JSON and docs routes.
    pub fn with_openapi(mut self, title: impl Into<String>, version: impl Into<String>) -> Self {
        self.openapi = Some(OpenApiOptions {
            title: title.into(),
            version: version.into(),
        });
        self
    }

    /// Registers a schema in the generated OpenAPI document.
    #[cfg(feature = "openapi")]
    pub fn with_schema<T>(mut self) -> Self
    where
        T: utoipa::ToSchema,
    {
        self.schemas.push(|document| document.schema::<T>());
        self
    }

    /// Enables the default HTTP tracing middleware on the composed router.
    pub fn with_tracing(mut self) -> Self {
        self.tracing = true;
        self
    }

    /// Applies production observability to the composed HTTP application.
    ///
    /// This merges observability routes such as `/metrics`, applies HTTP
    /// metrics when enabled, and applies the default HTTP tracing layer when
    /// [`Observability::tracing`] was configured.
    #[cfg(feature = "observability")]
    pub fn with_observability(mut self, observability: Observability) -> Self {
        self.observability = Some(observability);
        self
    }

    /// Merges an Axum router into the application built from the root module.
    ///
    /// This is the ergonomic builder-path equivalent of attaching a router
    /// through [`ApplicationHttpExt`] after bootstrapping an [`Application`].
    /// Routes declared by controllers and routes from every router passed here
    /// are composed before tracing, observability, or other builder-owned HTTP
    /// layers are applied.
    #[cfg(feature = "http")]
    pub fn with_router(mut self, router: Router) -> Self {
        self.routers.push(router);
        self
    }

    /// Mounts Nidus Dashboard into the composed HTTP application.
    #[cfg(feature = "dashboard")]
    pub fn with_dashboard(mut self, dashboard: NidusDashboard) -> Self {
        self.dashboard = Some(dashboard);
        self
    }

    /// Builds the application after merging an Axum router.
    ///
    /// This is a convenience for `Nidus::create::<AppModule>()
    /// .with_router(router).build().await`.
    #[cfg(feature = "http")]
    pub async fn build_with_router(self, router: Router) -> Result<HttpApplication> {
        self.with_router(router).build().await
    }

    /// Builds a composed HTTP application.
    #[cfg(feature = "http")]
    pub async fn build(mut self) -> Result<HttpApplication> {
        #[cfg(feature = "observability")]
        let started_at = std::time::Instant::now();
        let graph_result = ModuleGraph::from_root::<M>();
        #[cfg(feature = "observability")]
        if let Some(observability) = &self.observability {
            observability.record_module_graph_validation(
                if graph_result.is_ok() {
                    OperationStatus::Success
                } else {
                    OperationStatus::Failure
                },
                started_at.elapsed(),
            );
        }
        let graph = graph_result?;

        for module in graph.modules() {
            for registrar in module.provider_registrars() {
                registrar(&mut self.container)?;
            }
        }

        for module in graph.modules() {
            for initializer in module.async_initializers() {
                initializer(&mut self.container).await?;
            }
        }

        let router = self.build_router(&graph)?;
        #[cfg(feature = "dashboard")]
        if let Some(dashboard) = &self.dashboard {
            for route in self.dashboard_route_snapshots.drain(..) {
                dashboard.record_route_snapshot(route).await.map_err(|error| {
                    NidusError::ApplicationBuild {
                        message: error.to_string(),
                    }
                })?;
            }
        }
        Ok(Application::new(self.container, graph).with_router(router))
    }

    #[cfg(feature = "http")]
    fn build_router(&mut self, graph: &ModuleGraph) -> Result<Router> {
        let mut router = Router::new();
        let mut seen_routes = BTreeSet::new();

        #[cfg(feature = "openapi")]
        let mut openapi = self
            .openapi
            .as_ref()
            .map(|options| OpenApiDocument::new(&options.title, &options.version));

        for module in graph.modules() {
            for controller in module.controller_descriptors() {
                let routes = downcast_routes(controller.route_metadata())?;
                for route in &routes {
                    let full_path = route.try_full_path(controller.prefix()).map_err(|error| {
                        NidusError::ApplicationBuild {
                            message: error.to_string(),
                        }
                    })?;
                    let key = format!("{} {full_path}", route.method());
                    if !seen_routes.insert(key.clone()) {
                        return Err(NidusError::ApplicationBuild {
                            message: format!("duplicate route `{key}`"),
                        });
                    }
                    #[cfg(feature = "dashboard")]
                    self.dashboard_route_snapshots
                        .push(DashboardRouteSnapshot {
                            method: route.method().to_owned(),
                            path: full_path,
                            summary: route.summary().map(str::to_owned),
                            guards: route.guards().iter().map(|value| (*value).to_owned()).collect(),
                            pipes: route.pipes().iter().map(|value| (*value).to_owned()).collect(),
                            validates: route.validates(),
                        });
                }

                #[cfg(feature = "openapi")]
                if let Some(document) = openapi.take() {
                    openapi = Some(
                        document
                            .try_controller_routes(controller.prefix(), &routes)
                            .map_err(|error| NidusError::ApplicationBuild {
                                message: error.to_string(),
                            })?
                            .schemas_from_route_metadata(&routes),
                    );
                }

                let controller_router = downcast_router(controller.build_router(&self.container)?)?;
                router = router.merge(controller_router);
            }
        }

        for manual_router in self.routers.drain(..) {
            router = router.merge(manual_router);
        }

        #[cfg(feature = "dashboard")]
        if let Some(dashboard) = &self.dashboard {
            router = router.merge(dashboard.mounted_router());
        }

        #[cfg(feature = "openapi")]
        if let Some(mut document) = openapi {
            for schema in &self.schemas {
                document = schema(document);
            }
            router = router.merge(document.into_router());
        }

        #[cfg(feature = "observability")]
        if let Some(observability) = &self.observability {
            router = router.merge(observability.routes());
            if observability.http_metrics_enabled() {
                router = router.layer(observability.http_layer());
            }
        }

        #[cfg(feature = "observability")]
        if let Some(observability) = &self.observability
            && observability.tracing_enabled()
        {
            let mut logging =
                nidus_http::logging::LoggingConfig::production(observability.service_name());
            if let Some(version) = observability.version_label() {
                logging = logging.version(version);
            }
            if let Some(environment) = observability.environment_label() {
                logging = logging.environment(environment);
            }
            return Ok(router.layer(
                tower_http::trace::TraceLayer::new_for_http()
                    .make_span_with(nidus_http::logging::StructuredMakeSpan::new(logging)),
            ));
        }

        if self.tracing {
            router = router.layer(nidus_http::middleware::trace_layer());
        }

        Ok(router)
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct OpenApiOptions {
    title: String,
    version: String,
}

#[cfg(feature = "http")]
fn downcast_router(value: Box<dyn std::any::Any + Send + Sync>) -> Result<Router> {
    value
        .downcast::<Router>()
        .map(|router| *router)
        .map_err(|_| NidusError::ApplicationBuild {
            message: "controller returned an unexpected router type".to_owned(),
        })
}

#[cfg(all(feature = "http", feature = "openapi"))]
fn downcast_routes(
    value: Box<dyn std::any::Any + Send + Sync>,
) -> Result<Vec<nidus_http::router::RouteMetadata>> {
    value
        .downcast::<Vec<nidus_http::router::RouteMetadata>>()
        .map(|routes| *routes)
        .map_err(|_| NidusError::ApplicationBuild {
            message: "controller returned unexpected route metadata".to_owned(),
        })
}

#[cfg(all(feature = "http", not(feature = "openapi")))]
fn downcast_routes(
    value: Box<dyn std::any::Any + Send + Sync>,
) -> Result<Vec<nidus_http::router::RouteMetadata>> {
    value
        .downcast::<Vec<nidus_http::router::RouteMetadata>>()
        .map(|routes| *routes)
        .map_err(|_| NidusError::ApplicationBuild {
            message: "controller returned unexpected route metadata".to_owned(),
        })
}

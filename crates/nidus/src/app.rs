//! High-level application composition for the facade crate.

#[cfg(feature = "dashboard")]
use std::collections::BTreeMap;
#[cfg(feature = "http")]
use std::collections::BTreeSet;

pub use nidus_core::app::{ApplicationPlan, Resource};
use nidus_core::{Container, Module, Nidus, Result};
#[cfg(feature = "http")]
use nidus_core::{ModuleGraph, NidusError};

#[cfg(feature = "dashboard")]
use nidus_dashboard::{
    DashboardGraphEdge, DashboardGraphEdgeKind, DashboardGraphGroup, DashboardGraphNode,
    DashboardGraphNodeKind, DashboardGraphResponse, DashboardRouteSnapshot, NidusDashboard,
};
#[cfg(feature = "http")]
use nidus_http::{
    Router,
    server::{ApplicationHttpExt, HttpApplication},
};
#[cfg(feature = "observability")]
use nidus_observability::{Observability, OperationStatus};
#[cfg(feature = "openapi")]
use nidus_openapi::OpenApiDocument;

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
    overrides: std::collections::BTreeSet<&'static str>,
    lifecycle: nidus_core::LifecycleRunner,
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
            overrides: std::collections::BTreeSet::new(),
            lifecycle: nidus_core::LifecycleRunner::new(),
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

    /// Replaces a module provider before registration, initialization or controller construction.
    /// Replaced resources are externally owned; their original initializer and cleanup do not run.
    pub fn override_provider<T: Send + Sync + 'static>(mut self, value: T) -> Result<Self> {
        self.overrides.insert(std::any::type_name::<T>());
        self.container.override_singleton(value)?;
        Ok(self)
    }

    /// Registers an application lifecycle participant after module resources.
    pub fn lifecycle_hook<H: nidus_core::LifecycleHook>(mut self, hook: H) -> Self {
        self.lifecycle = self.lifecycle.hook(hook);
        self
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
    /// through [`ApplicationHttpExt`] after bootstrapping an [`Application`](nidus_core::Application).
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

    /// Builds a worker application using the shared provider/resource composition.
    /// HTTP controllers are intentionally not mounted on this explicit worker path.
    pub async fn build_application(mut self) -> Result<nidus_core::Application> {
        let graph = nidus_core::ModuleGraph::from_root::<M>()?;
        let mut plan = nidus_core::app::ApplicationPlan::prepare(
            graph,
            std::mem::take(&mut self.container),
            self.overrides,
        )
        .await?;
        plan.initialize().await?;
        Ok(plan.finish(self.lifecycle))
    }

    /// Starts a worker-only managed application; HTTP features are not required.
    pub async fn start_managed_worker(
        self,
        options: nidus_core::lifecycle::managed::ManagedOptions,
    ) -> std::result::Result<
        nidus_core::lifecycle::managed::Managed<nidus_core::Application>,
        std::sync::Arc<nidus_core::lifecycle::managed::Report>,
    >
    where
        M: Send + Sync + 'static,
    {
        nidus_core::lifecycle::managed::Managed::start(self.build_application(), options).await
    }

    /// Starts an in-memory managed application, preserving configured middleware.
    /// Pass the result to `TestApp::from_managed` for production-composition tests.
    #[cfg(feature = "http")]
    pub async fn build_managed(
        self,
        options: nidus_core::lifecycle::managed::ManagedOptions,
    ) -> std::result::Result<
        nidus_core::lifecycle::managed::Managed<HttpApplication>,
        std::sync::Arc<nidus_core::lifecycle::managed::Report>,
    >
    where
        M: Send + Sync + 'static,
    {
        nidus_core::lifecycle::managed::Managed::start(self.build(), options).await
    }

    /// Starts a managed HTTP application with tracked connections and bounded cleanup.
    #[cfg(feature = "http")]
    pub async fn start_managed(
        self,
        address: std::net::SocketAddr,
        options: nidus_core::lifecycle::managed::ManagedOptions,
    ) -> std::result::Result<
        nidus_core::lifecycle::managed::Managed<nidus_http::managed::ManagedHttp>,
        std::sync::Arc<nidus_core::lifecycle::managed::Report>,
    >
    where
        M: Send + Sync + 'static,
    {
        nidus_core::lifecycle::managed::Managed::start(
            async move {
                self.build()
                    .await
                    .map(|app| nidus_http::managed::ManagedHttp::new(app, address))
            },
            options,
        )
        .await
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

        nidus_http::composition::validate_routes(&graph)?;
        let mut plan = ApplicationPlan::prepare(
            graph,
            std::mem::take(&mut self.container),
            std::mem::take(&mut self.overrides),
        )
        .await?;
        plan.initialize().await?;
        let (mut plan, controllers) = nidus_http::composition::construct_controllers(plan).await?;
        let router = match controllers.and_then(|router| {
            nidus_http::composition::assemble(|| self.build_router(plan.graph(), router))
        }) {
            Ok(router) => router,
            Err(error) => return Err(plan.rollback(error).await),
        };
        #[cfg(feature = "dashboard")]
        let graph = plan.graph();
        #[cfg(feature = "dashboard")]
        if let Some(dashboard) = &self.dashboard {
            let snapshot = match dashboard_graph_from_module_graph(graph) {
                Ok(snapshot) => snapshot,
                Err(error) => return Err(plan.rollback(error).await),
            };
            dashboard.record_graph_snapshot(snapshot);
            for route in self.dashboard_route_snapshots.drain(..) {
                if let Err(error) = dashboard.record_route_snapshot(route).await {
                    return Err(plan
                        .rollback(NidusError::ApplicationBuild {
                            message: error.to_string(),
                        })
                        .await);
                }
            }
        }
        let application = plan.finish(self.lifecycle);
        let router =
            nidus_http::composition::application_context(router, application.shared_container());
        Ok(application.with_router(router))
    }

    #[cfg(feature = "http")]
    fn build_router(&mut self, graph: &ModuleGraph, mut router: Router) -> Result<Router> {
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
                    self.dashboard_route_snapshots.push(DashboardRouteSnapshot {
                        method: route.method().to_owned(),
                        path: full_path,
                        summary: route.summary().map(str::to_owned),
                        guards: route
                            .guards()
                            .iter()
                            .map(|value| (*value).to_owned())
                            .collect(),
                        pipes: route
                            .pipes()
                            .iter()
                            .map(|value| (*value).to_owned())
                            .collect(),
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

#[cfg(feature = "dashboard")]
fn dashboard_graph_from_module_graph(graph: &ModuleGraph) -> Result<DashboardGraphResponse> {
    let service_name = "nidus-app".to_owned();
    let runtime_id = format!("runtime:{service_name}");
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut groups = Vec::new();

    let module_ids = graph
        .modules()
        .map(|module| module_node_id(module.name()))
        .collect::<Vec<_>>();

    let mut runtime_counts = BTreeMap::new();
    runtime_counts.insert("modules".to_owned(), module_ids.len());
    runtime_counts.insert(
        "controllers".to_owned(),
        graph
            .modules()
            .map(|module| module.controllers().len())
            .sum(),
    );
    runtime_counts.insert(
        "providers".to_owned(),
        graph.modules().map(|module| module.providers().len()).sum(),
    );

    nodes.push(DashboardGraphNode {
        id: runtime_id.clone(),
        kind: DashboardGraphNodeKind::Runtime,
        label: service_name.clone(),
        summary: Some("validated Nidus module graph".to_owned()),
        status: Some("ready".to_owned()),
        counts: runtime_counts,
        metadata: BTreeMap::from([(
            "root".to_owned(),
            serde_json::Value::String("ModuleGraph".to_owned()),
        )]),
    });
    groups.push(DashboardGraphGroup {
        id: "modules".to_owned(),
        label: "Modules".to_owned(),
        nodes: module_ids,
    });

    for module in graph.modules() {
        let module_id = module_node_id(module.name());
        let mut counts = BTreeMap::new();
        counts.insert("imports".to_owned(), module.imports().len());
        counts.insert("providers".to_owned(), module.providers().len());
        counts.insert("controllers".to_owned(), module.controllers().len());
        counts.insert("exports".to_owned(), module.exports().len());

        nodes.push(DashboardGraphNode {
            id: module_id.clone(),
            kind: DashboardGraphNodeKind::Module,
            label: module.name().to_owned(),
            summary: Some(format!(
                "{} imports / {} controllers",
                module.imports().len(),
                module.controllers().len()
            )),
            status: None,
            counts,
            metadata: BTreeMap::from([
                (
                    "imports".to_owned(),
                    serde_json::to_value(module.imports()).unwrap_or(serde_json::Value::Null),
                ),
                (
                    "providers".to_owned(),
                    serde_json::to_value(module.providers()).unwrap_or(serde_json::Value::Null),
                ),
                (
                    "controllers".to_owned(),
                    serde_json::to_value(module.controllers()).unwrap_or(serde_json::Value::Null),
                ),
                (
                    "exports".to_owned(),
                    serde_json::to_value(module.exports()).unwrap_or(serde_json::Value::Null),
                ),
            ]),
        });
        edges.push(DashboardGraphEdge {
            id: format!("runtime-module:{module_id}"),
            kind: DashboardGraphEdgeKind::RuntimeModule,
            source: runtime_id.clone(),
            target: module_id.clone(),
            label: Some("module".to_owned()),
        });

        for import in module.imports() {
            edges.push(DashboardGraphEdge {
                id: format!("module-import:{module_id}:{import}"),
                kind: DashboardGraphEdgeKind::ModuleImport,
                source: module_id.clone(),
                target: module_node_id(import),
                label: Some("imports".to_owned()),
            });
        }

        for provider in module.providers() {
            let provider_id = provider_node_id(module.name(), provider);
            nodes.push(DashboardGraphNode {
                id: provider_id.clone(),
                kind: DashboardGraphNodeKind::Provider,
                label: provider.clone(),
                summary: Some(format!("provider in {}", module.name())),
                status: None,
                counts: BTreeMap::new(),
                metadata: BTreeMap::from([
                    (
                        "module".to_owned(),
                        serde_json::Value::String(module.name().to_owned()),
                    ),
                    (
                        "exported".to_owned(),
                        serde_json::Value::Bool(module.exports().contains(provider)),
                    ),
                ]),
            });
            edges.push(DashboardGraphEdge {
                id: format!("module-provider:{module_id}:{provider_id}"),
                kind: DashboardGraphEdgeKind::ModuleProvider,
                source: module_id.clone(),
                target: provider_id.clone(),
                label: Some("declares".to_owned()),
            });
            if module.exports().contains(provider) {
                edges.push(DashboardGraphEdge {
                    id: format!("module-export:{module_id}:{provider_id}"),
                    kind: DashboardGraphEdgeKind::ModuleExport,
                    source: module_id.clone(),
                    target: provider_id,
                    label: Some("exports".to_owned()),
                });
            }
        }

        for controller in module.controller_descriptors() {
            let controller_id = controller_node_id(module.name(), controller.name());
            let routes = downcast_routes(controller.route_metadata())?;
            let route_ids = routes
                .iter()
                .map(|route| {
                    let full_path = route.try_full_path(controller.prefix()).map_err(|error| {
                        NidusError::ApplicationBuild {
                            message: error.to_string(),
                        }
                    })?;
                    Ok(route_node_id(route.method(), &full_path))
                })
                .collect::<Result<Vec<_>>>()?;

            nodes.push(DashboardGraphNode {
                id: controller_id.clone(),
                kind: DashboardGraphNodeKind::Controller,
                label: controller.name().to_owned(),
                summary: Some(format!("{} routes", route_ids.len())),
                status: None,
                counts: BTreeMap::from([("routes".to_owned(), route_ids.len())]),
                metadata: BTreeMap::from([
                    (
                        "module".to_owned(),
                        serde_json::Value::String(module.name().to_owned()),
                    ),
                    (
                        "prefix".to_owned(),
                        serde_json::Value::String(controller.prefix().to_owned()),
                    ),
                ]),
            });
            edges.push(DashboardGraphEdge {
                id: format!("module-controller:{module_id}:{controller_id}"),
                kind: DashboardGraphEdgeKind::ModuleController,
                source: module_id.clone(),
                target: controller_id.clone(),
                label: Some("declares".to_owned()),
            });

            for route in routes {
                let full_path = route.try_full_path(controller.prefix()).map_err(|error| {
                    NidusError::ApplicationBuild {
                        message: error.to_string(),
                    }
                })?;
                let route_id = route_node_id(route.method(), &full_path);
                nodes.push(DashboardGraphNode {
                    id: route_id.clone(),
                    kind: DashboardGraphNodeKind::Route,
                    label: format!("{} {full_path}", route.method()),
                    summary: route.summary().map(str::to_owned),
                    status: None,
                    counts: BTreeMap::from([
                        ("guards".to_owned(), route.guards().len()),
                        ("pipes".to_owned(), route.pipes().len()),
                    ]),
                    metadata: BTreeMap::from([
                        (
                            "module".to_owned(),
                            serde_json::Value::String(module.name().to_owned()),
                        ),
                        (
                            "controller".to_owned(),
                            serde_json::Value::String(controller.name().to_owned()),
                        ),
                        (
                            "method".to_owned(),
                            serde_json::Value::String(route.method().to_owned()),
                        ),
                        (
                            "path".to_owned(),
                            serde_json::Value::String(full_path.clone()),
                        ),
                        (
                            "guards".to_owned(),
                            serde_json::to_value(route.guards()).unwrap_or(serde_json::Value::Null),
                        ),
                        (
                            "pipes".to_owned(),
                            serde_json::to_value(route.pipes()).unwrap_or(serde_json::Value::Null),
                        ),
                        (
                            "validates".to_owned(),
                            serde_json::Value::Bool(route.validates()),
                        ),
                    ]),
                });
                edges.push(DashboardGraphEdge {
                    id: format!("controller-route:{controller_id}:{route_id}"),
                    kind: DashboardGraphEdgeKind::ControllerRoute,
                    source: controller_id.clone(),
                    target: route_id,
                    label: Some(route.method().to_owned()),
                });
            }
        }
    }

    Ok(DashboardGraphResponse {
        service_name,
        generated_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default(),
        nodes,
        edges,
        groups,
    })
}

#[cfg(feature = "dashboard")]
fn module_node_id(module: &str) -> String {
    format!("module:{module}")
}

#[cfg(feature = "dashboard")]
fn provider_node_id(module: &str, provider: &str) -> String {
    format!("provider:{module}:{provider}")
}

#[cfg(feature = "dashboard")]
fn controller_node_id(module: &str, controller: &str) -> String {
    format!("controller:{module}:{controller}")
}

#[cfg(feature = "dashboard")]
fn route_node_id(method: &str, path: &str) -> String {
    format!("route:{method} {path}")
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct OpenApiOptions {
    title: String,
    version: String,
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

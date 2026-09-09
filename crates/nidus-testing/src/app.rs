use axum::{Extension, Router};
use http::Method;
use nidus_config::Config;
use nidus_core::{
    Application, Container, LifecycleHook, LifecycleRunner, Module, ModuleDefinition, ModuleGraph,
    RequestScope, Result,
};
use nidus_http::middleware::request_scope_layer;
use std::sync::Arc;

use crate::request::TestRequest;

/// In-memory test application backed by an Axum router.
///
/// `TestApp` drives requests through the router with Tower's in-memory service
/// path, so no TCP listener is started. Use it for handler, middleware, module,
/// and provider integration tests.
///
/// ```
/// use axum::{Json, Router, routing::get};
/// use http::StatusCode;
/// use nidus_testing::TestApp;
/// use serde_json::json;
///
/// async fn health() -> Json<serde_json::Value> {
///     Json(json!({ "ok": true }))
/// }
///
/// #[tokio::test]
/// async fn health_returns_json() {
///     let app = TestApp::from_router(
///         Router::new().route("/health", get(health)),
///     );
///
///     let response = app.get("/health").send().await;
///     response.assert_status(StatusCode::OK);
///     response.assert_json(json!({ "ok": true }));
/// }
/// ```
#[derive(Clone)]
pub struct TestApp {
    router: Router,
    container: Arc<Container>,
    config: Config,
    lifecycle: Arc<LifecycleRunner>,
    application: Option<Arc<Application>>,
    managed: Option<nidus_core::lifecycle::managed::Managed<nidus_http::server::HttpApplication>>,
}

impl TestApp {
    /// Creates a test application builder after validating a root Nidus module.
    pub fn bootstrap<M>() -> Result<TestAppBuilder>
    where
        M: Module,
    {
        Self::bootstrap_with_router::<M>(Router::new())
    }

    /// Creates a test application builder with a router after validating a root Nidus module.
    pub fn bootstrap_with_router<M>(router: Router) -> Result<TestAppBuilder>
    where
        M: Module,
    {
        let mut builder = Self::builder(router);
        let graph = ModuleGraph::from_root::<M>()?;
        nidus_http::composition::validate_routes(&graph)?;
        builder.graph = Some(graph);
        Ok(builder)
    }

    /// Creates a test application builder after validating an explicit module graph.
    pub fn bootstrap_with_modules<M, I>(modules: I) -> Result<TestAppBuilder>
    where
        M: Module,
        I: IntoIterator<Item = ModuleDefinition>,
    {
        Self::bootstrap_with_modules_and_router::<M, I>(modules, Router::new())
    }

    /// Creates a test application builder with a router after validating an explicit module graph.
    pub fn bootstrap_with_modules_and_router<M, I>(
        modules: I,
        router: Router,
    ) -> Result<TestAppBuilder>
    where
        M: Module,
        I: IntoIterator<Item = ModuleDefinition>,
    {
        let mut builder = Self::builder(router);
        let graph = ModuleGraph::from_root_and_modules::<M, I>(modules)?;
        nidus_http::composition::validate_routes(&graph)?;
        builder.graph = Some(graph);
        Ok(builder)
    }

    /// Creates a test application from an Axum router.
    ///
    /// This is the shortest path for HTTP-only tests. It installs an empty
    /// Nidus container extension so handlers that need the container can still
    /// extract it.
    pub fn from_router(router: Router) -> Self {
        let container = Arc::new(Container::new());
        Self {
            router: router.layer(Extension(Arc::clone(&container))),
            container,
            config: Config::new(),
            lifecycle: Arc::new(LifecycleRunner::new()),
            application: None,
            managed: None,
        }
    }

    /// Creates a configurable test application builder.
    pub fn builder(router: Router) -> TestAppBuilder {
        TestAppBuilder {
            router,
            container: Container::new(),
            config: Config::new(),
            lifecycle: LifecycleRunner::new(),
            config_override: false,
            request_scope: false,
            graph: None,
            overrides: Default::default(),
        }
    }

    /// Tests the fully configured production router with its original container and lifecycle.
    /// No middleware is rebuilt. Explicit shutdown remains required for owned resources.
    pub fn from_application(application: nidus_http::server::HttpApplication) -> Self {
        let (application, router) = application.into_parts();
        let container = application.shared_container();
        let config = Config::new();
        Self {
            router,
            container,
            config,
            lifecycle: Arc::new(LifecycleRunner::new()),
            application: Some(Arc::new(application)),
            managed: None,
        }
    }

    /// Tests a managed in-memory application with exactly-once shutdown ownership.
    pub fn from_managed(
        managed: nidus_core::lifecycle::managed::Managed<nidus_http::server::HttpApplication>,
    ) -> Self {
        let container = managed.target().application().shared_container();
        let config = Config::new();
        Self {
            router: managed.target().router().clone(),
            container,
            config,
            lifecycle: Arc::new(LifecycleRunner::new()),
            application: None,
            managed: Some(managed),
        }
    }

    /// Returns the managed report after joining cleanup, when this harness owns a managed application.
    pub async fn shutdown_report(&self) -> Option<Arc<nidus_core::lifecycle::managed::Report>> {
        match &self.managed {
            Some(managed) => Some(managed.shutdown().await),
            None => None,
        }
    }

    /// Starts a GET request.
    pub fn get(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::GET, path)
    }

    /// Starts a POST request.
    pub fn post(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::POST, path)
    }

    /// Starts a PUT request.
    pub fn put(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::PUT, path)
    }

    /// Starts a PATCH request.
    pub fn patch(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::PATCH, path)
    }

    /// Starts a DELETE request.
    pub fn delete(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::DELETE, path)
    }

    /// Starts a request with an arbitrary HTTP method.
    ///
    /// The returned [`TestRequest`] can set headers, query parameters, and body
    /// content before [`TestRequest::send`] executes it against the in-memory
    /// router.
    pub fn request(&self, method: Method, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), method, path.into())
            .supervised(self.managed.as_ref().map(|managed| managed.spawner()))
    }

    /// Resolves a provider from the test container.
    pub fn resolve<T>(&self) -> Result<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.container.resolve::<T>()
    }

    /// Creates a request scope for resolving request-lifetime providers in tests.
    pub fn request_scope(&self) -> RequestScope<'_> {
        self.container.request_scope()
    }

    /// Returns explicit test configuration overrides, not a snapshot of a configuration provider.
    /// For composed production configuration use `resolve::<Config>()`.
    /// Module-builder `config` overrides also replace the concrete `Config` provider
    /// before initialization; router-only helpers retain their separate harness configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Runs registered test shutdown lifecycle hooks.
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(managed) = &self.managed {
            let report = managed.shutdown().await;
            return if report.is_success() {
                Ok(())
            } else {
                Err(nidus_core::NidusError::ApplicationBuild {
                    message: format!(
                        "managed shutdown failed: {report:?}; use shutdown_report for sources"
                    ),
                })
            };
        }
        if let Some(application) = &self.application {
            application.shutdown().await
        } else {
            self.lifecycle.shutdown().await
        }
    }
}

/// Builder for in-memory test applications.
///
/// Use the builder when tests need provider overrides, request-scoped
/// providers, config overrides, or lifecycle hooks in addition to an Axum
/// router.
pub struct TestAppBuilder {
    router: Router,
    container: Container,
    config: Config,
    lifecycle: LifecycleRunner,
    config_override: bool,
    request_scope: bool,
    graph: Option<ModuleGraph>,
    overrides: std::collections::BTreeSet<&'static str>,
}

impl TestAppBuilder {
    /// Registers a provider in the test container.
    pub fn provider<T>(mut self, value: T) -> Result<Self>
    where
        T: Send + Sync + 'static,
    {
        self.container.register_singleton(value)?;
        Ok(self)
    }

    /// Registers a transient provider factory in the test container.
    pub fn transient_provider<T, F>(mut self, factory: F) -> Result<Self>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.container.register_transient::<T, F>(factory)?;
        Ok(self)
    }

    /// Registers a request-lifetime provider factory in the test container.
    pub fn request_provider<T, F>(mut self, factory: F) -> Result<Self>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.container.register_request::<T, F>(factory)?;
        Ok(self)
    }

    /// Registers a request-lifetime provider factory that resolves dependencies
    /// through the active request scope.
    ///
    /// This matches production request-scoped resolution. To exercise
    /// `RequestScoped<T>` extractors over HTTP in the test app, also call
    /// [`TestAppBuilder::with_request_scope`] so the request scope layer is
    /// installed on the router.
    pub fn request_scoped_provider<T, F>(mut self, factory: F) -> Result<Self>
    where
        T: Send + Sync + 'static,
        F: for<'scope> Fn(&RequestScope<'scope>) -> Result<T> + Send + Sync + 'static,
    {
        self.container.register_request_scoped::<T, F>(factory)?;
        Ok(self)
    }

    /// Installs the production request scope layer so `RequestScoped<T>`
    /// extractors resolve during HTTP integration tests.
    ///
    /// Without this, handlers that extract `RequestScoped<T>` reject with
    /// `500`/`request_scope_unavailable`. Register request providers with
    /// [`Self::request_provider`] or [`Self::request_scoped_provider`] first.
    pub fn with_request_scope(mut self) -> Self {
        self.request_scope = true;
        self
    }

    /// Overrides a provider in the test container.
    pub fn override_provider<T>(mut self, value: T) -> Result<Self>
    where
        T: Send + Sync + 'static,
    {
        self.overrides.insert(std::any::type_name::<T>());
        self.container.override_singleton(value)?;
        Ok(self)
    }

    /// Sets configuration overrides for the test application.
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self.config_override = true;
        self
    }

    /// Registers a lifecycle hook for the test application.
    pub fn lifecycle_hook<H>(mut self, hook: H) -> Self
    where
        H: LifecycleHook,
    {
        self.lifecycle = self.lifecycle.hook(hook);
        self
    }

    /// Builds a synchronous test application.
    /// Panics on module composition failure or async module initializers; use
    /// `try_build` for fallible synchronous composition or `build_started` for resources.
    pub fn build(self) -> TestApp {
        self.try_build()
            .expect("test application composition failed; use build_started for async modules")
    }

    /// Builds without running asynchronous initializers or startup hooks.
    pub fn try_build(mut self) -> Result<TestApp> {
        if let Some(graph) = self.graph.take() {
            if graph
                .modules()
                .any(|module| !module.async_initializers().is_empty())
            {
                return Err(nidus_core::NidusError::ApplicationBuild {
                    message: "async module initializers require build_started".to_owned(),
                });
            }
            let plan = self.plan(graph)?;
            let router = nidus_http::composition::assemble(|| {
                Ok(
                    nidus_http::composition::controller_router(plan.graph(), plan.container())?
                        .merge(self.router),
                )
            })?;
            return Ok(Self::finish_module(
                plan,
                router,
                self.lifecycle,
                self.request_scope,
                self.config,
            ));
        }
        let container = Arc::new(self.container);
        let mut router = self.router.layer(Extension(Arc::clone(&container)));
        if self.request_scope {
            router = router.layer(request_scope_layer(Arc::clone(&container)));
        }
        Ok(TestApp {
            router,
            container,
            config: self.config,
            lifecycle: Arc::new(self.lifecycle),
            application: None,
            managed: None,
        })
    }

    fn apply_config_override(&mut self) -> Result<()> {
        if self.config_override {
            self.container.override_singleton(self.config.clone())?;
            self.overrides.insert(std::any::type_name::<Config>());
        }
        Ok(())
    }

    fn plan(&mut self, graph: ModuleGraph) -> Result<nidus_core::app::ApplicationPlan> {
        self.apply_config_override()?;
        nidus_core::app::ApplicationPlan::new(
            graph,
            std::mem::take(&mut self.container),
            std::mem::take(&mut self.overrides),
        )
    }

    fn finish_module(
        plan: nidus_core::app::ApplicationPlan,
        router: Router,
        lifecycle: LifecycleRunner,
        request_scope: bool,
        config: Config,
    ) -> TestApp {
        use nidus_http::server::ApplicationHttpExt;
        let application = plan.finish(lifecycle);
        let router = if request_scope && !application.container().requires_request_scope() {
            router.layer(request_scope_layer(application.shared_container()))
        } else {
            router
        };
        let router =
            nidus_http::composition::application_context(router, application.shared_container());
        let mut app = TestApp::from_application(application.with_router(router));
        app.config = config;
        app
    }

    async fn build_initialized(mut self) -> Result<TestApp> {
        if let Some(graph) = self.graph.take() {
            self.apply_config_override()?;
            let mut plan = nidus_core::ApplicationPlan::prepare(
                graph,
                std::mem::take(&mut self.container),
                std::mem::take(&mut self.overrides),
            )
            .await?;
            plan.initialize().await?;
            let (mut plan, controllers) =
                nidus_http::composition::construct_controllers(plan).await?;
            let router = match controllers.and_then(|router| {
                nidus_http::composition::assemble(|| Ok(router.merge(self.router)))
            }) {
                Ok(router) => router,
                Err(error) => return Err(plan.rollback(error).await),
            };
            let app = Self::finish_module(
                plan,
                router,
                self.lifecycle,
                self.request_scope,
                self.config,
            );
            return Ok(app);
        }
        self.try_build()
    }
    /// Initializes modules and runs low-level startup hooks.
    /// For cancellation-safe startup and exactly-once shutdown, use `build_managed`.
    pub async fn build_started(self) -> Result<TestApp> {
        let app = self.build_initialized().await?;
        if let Some(application) = &app.application {
            application.lifecycle().startup().await?;
        } else {
            app.lifecycle.startup().await?;
        }
        Ok(app)
    }

    /// Composes a module-based test application under the managed lifecycle owner.
    pub async fn build_managed(
        self,
        options: nidus_core::lifecycle::managed::ManagedOptions,
    ) -> std::result::Result<TestApp, Arc<nidus_core::lifecycle::managed::Report>> {
        use nidus_http::server::ApplicationHttpExt;
        let config = self.config.clone();
        let managed = nidus_core::lifecycle::managed::Managed::start(
            async move {
                if self.graph.is_none() {
                    return Err(nidus_core::NidusError::ApplicationBuild {
                        message: "build_managed requires module bootstrap".to_owned(),
                    });
                }
                let mut app = self.build_initialized().await?;
                let application =
                    Arc::try_unwrap(app.application.take().expect("module application"))
                        .unwrap_or_else(|_| unreachable!("new application has no clones"));
                Ok(application.with_router(app.router))
            },
            options,
        )
        .await?;
        let mut app = TestApp::from_managed(managed);
        app.config = config;
        Ok(app)
    }
}

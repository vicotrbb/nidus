use axum::{Extension, Router};
use http::Method;
use nidus_config::Config;
use nidus_core::{
    Container, LifecycleHook, LifecycleRunner, Module, ModuleDefinition, Nidus, RequestScope,
    Result,
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
        Nidus::bootstrap::<M>()?;
        Ok(Self::builder(router))
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
        Nidus::bootstrap_with_modules::<M, I>(modules)?;
        Ok(Self::builder(router))
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
        }
    }

    /// Creates a configurable test application builder.
    pub fn builder(router: Router) -> TestAppBuilder {
        TestAppBuilder {
            router,
            container: Container::new(),
            config: Config::new(),
            lifecycle: LifecycleRunner::new(),
            request_scope: false,
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

    /// Returns test configuration overrides.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Runs registered test shutdown lifecycle hooks.
    pub async fn shutdown(&self) -> Result<()> {
        self.lifecycle.shutdown().await
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
    request_scope: bool,
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
        self.container.override_singleton(value)?;
        Ok(self)
    }

    /// Sets configuration overrides for the test application.
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
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

    /// Builds the test application.
    pub fn build(self) -> TestApp {
        let container = Arc::new(self.container);
        let mut router = self.router.layer(Extension(Arc::clone(&container)));
        if self.request_scope {
            router = router.layer(request_scope_layer(Arc::clone(&container)));
        }
        TestApp {
            router,
            container,
            config: self.config,
            lifecycle: Arc::new(self.lifecycle),
        }
    }

    /// Runs startup hooks and builds the test application.
    pub async fn build_started(self) -> Result<TestApp> {
        self.lifecycle.startup().await?;
        Ok(self.build())
    }
}

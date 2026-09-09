//! Application bootstrap primitives.

mod plan;
pub use plan::{ApplicationPlan, Resource};
pub(crate) use plan::{ResourceCleanup, initialize_resource, resource_cleanup};

use std::sync::Arc;

use crate::{Container, LifecycleRunner, Module, ModuleDefinition, ModuleGraph, Result};

/// Bootstrapped Nidus application.
pub struct Application {
    container: Arc<Container>,
    modules: ModuleGraph,
    lifecycle: LifecycleRunner,
}

impl Application {
    /// Creates an application from an already validated container and graph.
    pub fn new(container: Container, modules: ModuleGraph) -> Self {
        Self {
            container: Arc::new(container),
            modules,
            lifecycle: LifecycleRunner::empty(),
        }
    }

    /// Creates an application from an already validated container, graph, and lifecycle runner.
    pub fn with_lifecycle(
        container: Container,
        modules: ModuleGraph,
        lifecycle: LifecycleRunner,
    ) -> Self {
        Self {
            container: Arc::new(container),
            modules,
            lifecycle,
        }
    }

    /// Returns the application dependency container.
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// Clones the shared application container used by requests and test resolution.
    pub fn shared_container(&self) -> Arc<Container> {
        Arc::clone(&self.container)
    }

    /// Clones the low-level lifecycle hooks, retaining their resource instances.
    pub fn lifecycle(&self) -> LifecycleRunner {
        self.lifecycle.clone()
    }

    /// Returns the validated module graph.
    pub fn modules(&self) -> &ModuleGraph {
        &self.modules
    }

    /// Runs every application shutdown hook in reverse registration order.
    ///
    /// If hooks fail, cleanup continues and the first shutdown error is
    /// returned after all hooks have been attempted.
    pub async fn shutdown(&self) -> Result<()> {
        self.lifecycle.shutdown().await
    }
}

/// Framework bootstrap entrypoint.
pub struct Nidus;

impl Nidus {
    /// Bootstraps a Nidus application from a root module definition.
    ///
    /// The module graph is validated and the container is populated with the root
    /// module graph's typed providers. Synchronous providers are registered here;
    /// providers that require async initialization are not run by this synchronous
    /// entrypoint and need [`Nidus::bootstrap_with_lifecycle`] (or the facade
    /// builder) to construct.
    pub fn bootstrap<M: Module>() -> Result<Application> {
        let graph = ModuleGraph::from_root::<M>()?;
        let mut container = Container::new();
        graph.register_providers(&mut container)?;
        Ok(Application::new(container, graph))
    }

    /// Bootstraps a Nidus application from a root module and explicit graph definitions.
    ///
    /// Like [`Nidus::bootstrap`], this validates the graph and registers synchronous
    /// typed providers. Async provider initializers are not run by this synchronous
    /// entrypoint.
    pub fn bootstrap_with_modules<M, I>(modules: I) -> Result<Application>
    where
        M: Module,
        I: IntoIterator<Item = ModuleDefinition>,
    {
        let graph = ModuleGraph::from_root_and_modules::<M, I>(modules)?;
        let mut container = Container::new();
        graph.register_providers(&mut container)?;
        Ok(Application::new(container, graph))
    }

    /// Bootstraps a Nidus application and runs startup lifecycle hooks.
    ///
    /// Typed providers are registered and async provider initializers run before
    /// startup hooks, so hooks can resolve fully initialized providers.
    pub async fn bootstrap_with_lifecycle<M: Module>(
        lifecycle: LifecycleRunner,
    ) -> Result<Application> {
        let graph = ModuleGraph::from_root::<M>()?;
        let mut plan =
            ApplicationPlan::prepare(graph, Container::new(), Default::default()).await?;
        plan.initialize().await?;
        let application = plan.finish(lifecycle);
        application.lifecycle.startup().await?;
        Ok(application)
    }

    /// Bootstraps a Nidus application from an explicit module graph and runs startup hooks.
    ///
    /// Typed providers are registered and async provider initializers run before
    /// startup hooks, so hooks can resolve fully initialized providers.
    pub async fn bootstrap_with_modules_and_lifecycle<M, I>(
        modules: I,
        lifecycle: LifecycleRunner,
    ) -> Result<Application>
    where
        M: Module,
        I: IntoIterator<Item = ModuleDefinition>,
    {
        let graph = ModuleGraph::from_root_and_modules::<M, I>(modules)?;
        let mut plan =
            ApplicationPlan::prepare(graph, Container::new(), Default::default()).await?;
        plan.initialize().await?;
        let application = plan.finish(lifecycle);
        application.lifecycle.startup().await?;
        Ok(application)
    }
}

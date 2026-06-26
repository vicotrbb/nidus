//! Application bootstrap primitives.

use crate::{Container, LifecycleRunner, Module, ModuleDefinition, ModuleGraph, Result};

/// Bootstrapped Nidus application.
pub struct Application {
    container: Container,
    modules: ModuleGraph,
    lifecycle: LifecycleRunner,
}

impl Application {
    /// Creates an application from an already validated container and graph.
    pub fn new(container: Container, modules: ModuleGraph) -> Self {
        Self {
            container,
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
            container,
            modules,
            lifecycle,
        }
    }

    /// Returns the application dependency container.
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// Returns the validated module graph.
    pub fn modules(&self) -> &ModuleGraph {
        &self.modules
    }

    /// Runs application shutdown hooks.
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
        register_module_providers(&mut container, &graph)?;
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
        register_module_providers(&mut container, &graph)?;
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
        let mut container = Container::new();
        register_module_providers(&mut container, &graph)?;
        initialize_module_providers(&mut container, &graph).await?;
        lifecycle.startup().await?;
        Ok(Application::with_lifecycle(container, graph, lifecycle))
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
        let mut container = Container::new();
        register_module_providers(&mut container, &graph)?;
        initialize_module_providers(&mut container, &graph).await?;
        lifecycle.startup().await?;
        Ok(Application::with_lifecycle(container, graph, lifecycle))
    }
}

fn register_module_providers(container: &mut Container, graph: &ModuleGraph) -> Result<()> {
    for module in graph.modules() {
        for registrar in module.provider_registrars() {
            registrar(container)?;
        }
    }
    Ok(())
}

async fn initialize_module_providers(container: &mut Container, graph: &ModuleGraph) -> Result<()> {
    for module in graph.modules() {
        for initializer in module.async_initializers() {
            initializer(container).await?;
        }
    }
    Ok(())
}

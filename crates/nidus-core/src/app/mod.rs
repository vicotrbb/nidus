//! Application bootstrap primitives.

use crate::{Container, LifecycleRunner, Module, ModuleGraph, Result};

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
    pub fn bootstrap<M: Module>() -> Result<Application> {
        let graph = ModuleGraph::from_modules([M::definition()])?;
        Ok(Application::new(Container::new(), graph))
    }

    /// Bootstraps a Nidus application and runs startup lifecycle hooks.
    pub async fn bootstrap_with_lifecycle<M: Module>(
        lifecycle: LifecycleRunner,
    ) -> Result<Application> {
        let graph = ModuleGraph::from_modules([M::definition()])?;
        lifecycle.startup().await?;
        Ok(Application::with_lifecycle(
            Container::new(),
            graph,
            lifecycle,
        ))
    }
}

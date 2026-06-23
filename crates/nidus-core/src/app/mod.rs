//! Application bootstrap primitives.

use crate::{Container, Module, ModuleGraph, Result};

/// Bootstrapped Nidus application.
pub struct Application {
    container: Container,
    modules: ModuleGraph,
}

impl Application {
    /// Creates an application from an already validated container and graph.
    pub fn new(container: Container, modules: ModuleGraph) -> Self {
        Self { container, modules }
    }

    /// Returns the application dependency container.
    pub fn container(&self) -> &Container {
        &self.container
    }

    /// Returns the validated module graph.
    pub fn modules(&self) -> &ModuleGraph {
        &self.modules
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
}

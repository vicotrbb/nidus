//! Module graph primitives.

mod graph;

pub use graph::ModuleGraph;

/// A Rust type that describes a Nidus module.
pub trait Module {
    /// Returns this module's static definition.
    fn definition() -> ModuleDefinition;
}

/// Explicit module metadata used to validate application structure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleDefinition {
    name: String,
    imports: Vec<String>,
    providers: Vec<String>,
    controllers: Vec<String>,
    exports: Vec<String>,
}

impl ModuleDefinition {
    /// Returns the module name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns explicit imported modules.
    pub fn imports(&self) -> &[String] {
        &self.imports
    }

    /// Returns providers owned by the module.
    pub fn providers(&self) -> &[String] {
        &self.providers
    }

    /// Returns controllers owned by the module.
    pub fn controllers(&self) -> &[String] {
        &self.controllers
    }

    /// Returns providers exported to importing modules.
    pub fn exports(&self) -> &[String] {
        &self.exports
    }
}

/// Builder for explicit module definitions.
#[derive(Debug)]
pub struct ModuleBuilder {
    definition: ModuleDefinition,
}

impl ModuleBuilder {
    /// Starts a module definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            definition: ModuleDefinition {
                name: name.into(),
                imports: Vec::new(),
                providers: Vec::new(),
                controllers: Vec::new(),
                exports: Vec::new(),
            },
        }
    }

    /// Adds an explicit module import.
    pub fn import(mut self, name: impl Into<String>) -> Self {
        self.definition.imports.push(name.into());
        self
    }

    /// Adds a provider declaration.
    pub fn provider(mut self, name: impl Into<String>) -> Self {
        self.definition.providers.push(name.into());
        self
    }

    /// Adds a controller declaration.
    pub fn controller(mut self, name: impl Into<String>) -> Self {
        self.definition.controllers.push(name.into());
        self
    }

    /// Adds a provider export declaration.
    pub fn export(mut self, name: impl Into<String>) -> Self {
        self.definition.exports.push(name.into());
        self
    }

    /// Completes the module definition.
    pub fn build(self) -> ModuleDefinition {
        self.definition
    }
}

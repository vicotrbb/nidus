//! Module graph primitives.

mod graph;

use std::{any::Any, future::Future, pin::Pin};

use crate::{Container, Result};

pub use graph::ModuleGraph;

/// A Rust type that describes a Nidus module.
pub trait Module {
    /// Returns this module's static definition.
    fn definition() -> ModuleDefinition;
}

/// Registers a provider type with a dependency container.
pub trait ProviderRegistrant {
    /// Registers this provider type.
    fn register_provider(container: &mut Container) -> Result<()>;
}

/// Type-erased controller descriptor used by feature crates to assemble HTTP routers.
pub trait ControllerRegistrant {
    /// Returns the controller type name.
    fn controller_name() -> &'static str;

    /// Returns the controller route prefix.
    fn controller_prefix() -> &'static str;

    /// Builds the controller router as a type-erased value.
    fn build_router(container: &Container) -> Result<Box<dyn Any + Send + Sync>>;

    /// Returns generated controller route metadata as a type-erased value.
    fn route_metadata() -> Box<dyn Any + Send + Sync>;
}

/// Registers a provider against a container.
pub type ProviderRegistrar = fn(&mut Container) -> Result<()>;

/// Creates an imported module definition.
pub type ModuleDefinitionFactory = fn() -> ModuleDefinition;

/// Initializes an async provider during application bootstrap.
pub type AsyncProviderInitializer =
    for<'a> fn(&'a mut Container) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Type-erased controller runtime hooks.
#[derive(Clone, Copy)]
pub struct ControllerDescriptor {
    name: &'static str,
    prefix: fn() -> &'static str,
    build_router: fn(&Container) -> Result<Box<dyn Any + Send + Sync>>,
    route_metadata: fn() -> Box<dyn Any + Send + Sync>,
}

impl ControllerDescriptor {
    /// Creates a descriptor for a typed controller.
    pub fn new<C>() -> Self
    where
        C: ControllerRegistrant,
    {
        Self {
            name: C::controller_name(),
            prefix: C::controller_prefix,
            build_router: C::build_router,
            route_metadata: C::route_metadata,
        }
    }

    /// Returns the controller type name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the controller route prefix.
    pub fn prefix(&self) -> &'static str {
        (self.prefix)()
    }

    /// Builds the type-erased controller router.
    pub fn build_router(&self, container: &Container) -> Result<Box<dyn Any + Send + Sync>> {
        (self.build_router)(container)
    }

    /// Returns type-erased generated route metadata.
    pub fn route_metadata(&self) -> Box<dyn Any + Send + Sync> {
        (self.route_metadata)()
    }
}

/// Explicit module metadata used to validate application structure.
#[derive(Clone)]
pub struct ModuleDefinition {
    name: String,
    imports: Vec<String>,
    providers: Vec<String>,
    controllers: Vec<String>,
    exports: Vec<String>,
    import_factories: Vec<ModuleDefinitionFactory>,
    provider_registrars: Vec<ProviderRegistrar>,
    controller_descriptors: Vec<ControllerDescriptor>,
    async_initializers: Vec<AsyncProviderInitializer>,
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

    /// Returns typed import factories declared by this module.
    pub fn import_factories(&self) -> &[ModuleDefinitionFactory] {
        &self.import_factories
    }

    /// Returns provider registration callbacks declared by this module.
    pub fn provider_registrars(&self) -> &[ProviderRegistrar] {
        &self.provider_registrars
    }

    /// Returns controller runtime descriptors declared by this module.
    pub fn controller_descriptors(&self) -> &[ControllerDescriptor] {
        &self.controller_descriptors
    }

    /// Returns async provider initializers declared by this module.
    pub fn async_initializers(&self) -> &[AsyncProviderInitializer] {
        &self.async_initializers
    }
}

impl std::fmt::Debug for ModuleDefinition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModuleDefinition")
            .field("name", &self.name)
            .field("imports", &self.imports)
            .field("providers", &self.providers)
            .field("controllers", &self.controllers)
            .field("exports", &self.exports)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ModuleDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.imports == other.imports
            && self.providers == other.providers
            && self.controllers == other.controllers
            && self.exports == other.exports
    }
}

impl Eq for ModuleDefinition {}

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
                import_factories: Vec::new(),
                provider_registrars: Vec::new(),
                controller_descriptors: Vec::new(),
                async_initializers: Vec::new(),
            },
        }
    }

    /// Adds an explicit module import.
    pub fn import(mut self, name: impl Into<String>) -> Self {
        self.definition.imports.push(name.into());
        self
    }

    /// Adds a typed module import.
    pub fn import_typed<M>(mut self) -> Self
    where
        M: Module,
    {
        let definition = M::definition();
        self.definition.imports.push(definition.name().to_owned());
        self.definition.import_factories.push(M::definition);
        self
    }

    /// Adds a provider declaration.
    pub fn provider(mut self, name: impl Into<String>) -> Self {
        self.definition.providers.push(name.into());
        self
    }

    /// Adds a typed provider declaration and registration callback.
    pub fn provider_typed<P>(mut self) -> Self
    where
        P: ProviderRegistrant,
    {
        self.definition.providers.push(
            std::any::type_name::<P>()
                .rsplit("::")
                .next()
                .unwrap()
                .to_owned(),
        );
        self.definition
            .provider_registrars
            .push(P::register_provider);
        self
    }

    /// Adds a controller declaration.
    pub fn controller(mut self, name: impl Into<String>) -> Self {
        self.definition.controllers.push(name.into());
        self
    }

    /// Adds a typed controller declaration and runtime descriptor.
    pub fn controller_typed<C>(mut self) -> Self
    where
        C: ControllerRegistrant,
    {
        self.definition
            .controllers
            .push(C::controller_name().to_owned());
        self.definition
            .controller_descriptors
            .push(ControllerDescriptor::new::<C>());
        self
    }

    /// Adds a provider export declaration.
    pub fn export(mut self, name: impl Into<String>) -> Self {
        self.definition.exports.push(name.into());
        self
    }

    /// Adds a typed provider export declaration.
    pub fn export_typed<P>(mut self) -> Self {
        self.definition.exports.push(
            std::any::type_name::<P>()
                .rsplit("::")
                .next()
                .unwrap()
                .to_owned(),
        );
        self
    }

    /// Adds an async provider initializer.
    pub fn async_initializer(mut self, initializer: AsyncProviderInitializer) -> Self {
        self.definition.async_initializers.push(initializer);
        self
    }

    /// Completes the module definition.
    pub fn build(self) -> ModuleDefinition {
        self.definition
    }
}

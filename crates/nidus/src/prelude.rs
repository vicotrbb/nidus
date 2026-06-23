//! Common imports for Nidus applications.

pub use nidus_core::{
    Application, Container, Factory, Inject, Lazy, Module, ModuleBuilder, ModuleDefinition,
    ModuleGraph, Nidus, NidusError, Optional, Provider, ProviderEntry, ProviderLifetime, Result,
    Scoped,
};
pub use nidus_macros::{
    controller, delete, get, guard, injectable, module, patch, pipe, post, put, routes,
};

#[cfg(feature = "http")]
pub use nidus_http::{
    controller::Controller,
    router::{RouteDefinition, RouteMetadata},
};

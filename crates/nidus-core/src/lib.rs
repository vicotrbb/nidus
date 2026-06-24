#![deny(missing_docs)]

//! Core application, module, provider, and dependency injection primitives.

pub mod app;
pub mod container;
pub mod error;
pub mod lifecycle;
pub mod module;
pub mod provider;

pub use app::{Application, Nidus};
pub use container::{
    Container, Factory, Inject, Lazy, Optional, RequestScope, Scoped, SharedRequestScope,
};
pub use error::{NidusError, Result};
pub use lifecycle::{LifecycleHook, LifecycleRunner};
pub use module::{Module, ModuleBuilder, ModuleDefinition, ModuleGraph};
pub use provider::{Provider, ProviderEntry, ProviderLifetime};

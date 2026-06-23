//! Provider registration primitives.

use std::{
    any::Any,
    sync::{Arc, Mutex},
};

use crate::{Container, Result};

/// Provider creation and reuse strategy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderLifetime {
    /// Create once and reuse for all resolutions.
    Singleton,
    /// Create a fresh value on every resolution.
    Transient,
    /// Create per request when request scopes are enabled.
    Request,
}

/// Marker trait for injectable provider values.
pub trait Provider: Send + Sync + 'static {}

impl<T> Provider for T where T: Send + Sync + 'static {}

type ErasedProvider = dyn Any + Send + Sync;
type ProviderFactory = dyn Fn(&Container) -> Result<Arc<ErasedProvider>> + Send + Sync;

/// A typed provider registration stored by the container.
pub struct ProviderEntry {
    type_name: &'static str,
    lifetime: ProviderLifetime,
    factory: Arc<ProviderFactory>,
    singleton: Mutex<Option<Arc<ErasedProvider>>>,
}

impl ProviderEntry {
    /// Creates a provider entry from an erased factory.
    pub fn new(
        type_name: &'static str,
        lifetime: ProviderLifetime,
        factory: Arc<ProviderFactory>,
    ) -> Self {
        Self {
            type_name,
            lifetime,
            factory,
            singleton: Mutex::new(None),
        }
    }

    /// Returns the registered provider type name.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the configured provider lifetime.
    pub fn lifetime(&self) -> ProviderLifetime {
        self.lifetime
    }

    pub(crate) fn resolve_erased(&self, container: &Container) -> Result<Arc<ErasedProvider>> {
        match self.lifetime {
            ProviderLifetime::Singleton => {
                if let Some(instance) = self
                    .singleton
                    .lock()
                    .expect("singleton mutex poisoned")
                    .clone()
                {
                    return Ok(instance);
                }

                let instance = (self.factory)(container)?;
                let mut singleton = self.singleton.lock().expect("singleton mutex poisoned");
                Ok(singleton.get_or_insert_with(|| instance).clone())
            }
            ProviderLifetime::Transient | ProviderLifetime::Request => (self.factory)(container),
        }
    }
}

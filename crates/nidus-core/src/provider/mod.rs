//! Provider registration primitives.

use std::{
    any::Any,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{Container, NidusError, RequestScope, Result};

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
type RequestProviderFactory =
    dyn for<'scope> Fn(&RequestScope<'scope>) -> Result<Arc<ErasedProvider>> + Send + Sync;

/// A typed provider registration stored by the container.
pub struct ProviderEntry {
    type_name: &'static str,
    lifetime: ProviderLifetime,
    factory: Arc<ProviderFactory>,
    request_factory: Option<Arc<RequestProviderFactory>>,
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
            request_factory: None,
            singleton: Mutex::new(None),
        }
    }

    /// Creates a request-scoped provider entry from an erased request-scope factory.
    pub fn new_request_scoped(
        type_name: &'static str,
        factory: Arc<ProviderFactory>,
        request_factory: Arc<RequestProviderFactory>,
    ) -> Self {
        Self {
            type_name,
            lifetime: ProviderLifetime::Request,
            factory,
            request_factory: Some(request_factory),
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
                if let Some(instance) = lock_unpoisoned(&self.singleton).clone() {
                    return Ok(instance);
                }

                let instance = self.create_erased(container)?;
                let mut singleton = lock_unpoisoned(&self.singleton);
                Ok(singleton.get_or_insert_with(|| instance).clone())
            }
            ProviderLifetime::Transient | ProviderLifetime::Request => {
                self.create_erased(container)
            }
        }
    }

    pub(crate) fn resolve_erased_in_scope(
        &self,
        scope: &RequestScope<'_>,
    ) -> Result<Arc<ErasedProvider>> {
        match self.lifetime {
            ProviderLifetime::Request => self.create_erased_in_scope(scope),
            ProviderLifetime::Singleton | ProviderLifetime::Transient => {
                self.resolve_erased(scope.container())
            }
        }
    }

    fn create_erased(&self, container: &Container) -> Result<Arc<ErasedProvider>> {
        (self.factory)(container).map_err(|source| NidusError::ProviderFactory {
            type_name: self.type_name,
            source: Box::new(source),
        })
    }

    fn create_erased_in_scope(&self, scope: &RequestScope<'_>) -> Result<Arc<ErasedProvider>> {
        if let Some(factory) = &self.request_factory {
            factory(scope).map_err(|source| NidusError::ProviderFactory {
                type_name: self.type_name,
                source: Box::new(source),
            })
        } else {
            self.create_erased(scope.container())
        }
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{
        any::{Any, type_name},
        sync::Arc,
        thread,
    };

    use super::{ProviderEntry, ProviderLifetime};
    use crate::Container;

    #[test]
    fn singleton_provider_recovers_from_poisoned_cache() {
        let provider = Arc::new(ProviderEntry::new(
            type_name::<String>(),
            ProviderLifetime::Singleton,
            Arc::new(|_container| Ok(Arc::new("ready".to_owned()) as Arc<dyn Any + Send + Sync>)),
        ));
        let poisoned_provider = Arc::clone(&provider);

        let panic = thread::spawn(move || {
            let _singleton = poisoned_provider.singleton.lock().unwrap();
            panic!("poison singleton cache");
        });
        assert!(panic.join().is_err());

        let value = provider
            .resolve_erased(&Container::new())
            .unwrap()
            .downcast::<String>()
            .unwrap();
        assert_eq!(&*value, "ready");
    }
}

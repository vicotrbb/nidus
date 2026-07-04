//! Provider registration primitives.

use std::{
    any::{Any, TypeId},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock},
};

use crate::{Container, NidusError, RequestScope, Result, resolution};

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
    type_id: TypeId,
    type_name: &'static str,
    lifetime: ProviderLifetime,
    factory: Arc<ProviderFactory>,
    request_factory: Option<Arc<RequestProviderFactory>>,
    singleton: Mutex<SingletonState>,
    singleton_ready: Condvar,
    // Lock-free read path for constructed singletons. Set exactly once, after a
    // factory succeeds; failed or panicking factories leave it empty so the
    // `singleton` state machine keeps its retry semantics.
    singleton_cache: OnceLock<Arc<ErasedProvider>>,
}

enum SingletonState {
    Empty,
    Initializing,
    Ready(Arc<ErasedProvider>),
}

impl ProviderEntry {
    /// Creates a provider entry from an erased factory.
    pub fn new(
        type_id: TypeId,
        type_name: &'static str,
        lifetime: ProviderLifetime,
        factory: Arc<ProviderFactory>,
    ) -> Self {
        Self {
            type_id,
            type_name,
            lifetime,
            factory,
            request_factory: None,
            singleton: Mutex::new(SingletonState::Empty),
            singleton_ready: Condvar::new(),
            singleton_cache: OnceLock::new(),
        }
    }

    /// Creates a request-scoped provider entry from an erased request-scope factory.
    pub fn new_request_scoped(
        type_id: TypeId,
        type_name: &'static str,
        factory: Arc<ProviderFactory>,
        request_factory: Arc<RequestProviderFactory>,
    ) -> Self {
        Self {
            type_id,
            type_name,
            lifetime: ProviderLifetime::Request,
            factory,
            request_factory: Some(request_factory),
            singleton: Mutex::new(SingletonState::Empty),
            singleton_ready: Condvar::new(),
            singleton_cache: OnceLock::new(),
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
            ProviderLifetime::Singleton => self.resolve_singleton(container),
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

    fn resolve_singleton(&self, container: &Container) -> Result<Arc<ErasedProvider>> {
        if let Some(instance) = self.singleton_cache.get() {
            return Ok(Arc::clone(instance));
        }
        loop {
            let mut singleton = lock_unpoisoned(&self.singleton);
            match &*singleton {
                SingletonState::Ready(instance) => return Ok(Arc::clone(instance)),
                SingletonState::Initializing => {
                    if resolution::is_active(self.type_id) {
                        return Err(NidusError::CircularProviderResolution {
                            type_name: self.type_name,
                        });
                    }
                    drop(wait_unpoisoned(&self.singleton_ready, singleton));
                }
                SingletonState::Empty => {
                    let _guard = resolution::enter(self.type_id, self.type_name)?;
                    *singleton = SingletonState::Initializing;
                    drop(singleton);

                    let instance =
                        match catch_unwind(AssertUnwindSafe(|| self.create_erased(container))) {
                            Ok(outcome) => outcome,
                            Err(panic_payload) => {
                                let mut singleton = lock_unpoisoned(&self.singleton);
                                *singleton = SingletonState::Empty;
                                self.singleton_ready.notify_all();
                                drop(singleton);
                                std::panic::resume_unwind(panic_payload);
                            }
                        };
                    let mut singleton = lock_unpoisoned(&self.singleton);
                    match instance {
                        Ok(instance) => {
                            *singleton = SingletonState::Ready(Arc::clone(&instance));
                            let _ = self.singleton_cache.set(Arc::clone(&instance));
                            self.singleton_ready.notify_all();
                            return Ok(instance);
                        }
                        Err(error) => {
                            *singleton = SingletonState::Empty;
                            self.singleton_ready.notify_all();
                            return Err(error);
                        }
                    }
                }
            }
        }
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

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
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
    fn singleton_provider_reuses_the_constructed_instance() {
        let provider = ProviderEntry::new(
            std::any::TypeId::of::<String>(),
            type_name::<String>(),
            ProviderLifetime::Singleton,
            Arc::new(|_container| Ok(Arc::new("ready".to_owned()) as Arc<dyn Any + Send + Sync>)),
        );
        let container = Container::new();

        let first = provider.resolve_erased(&container).unwrap();
        let second = provider.resolve_erased(&container).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn singleton_provider_retries_after_factory_error() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let failed_once = Arc::new(AtomicBool::new(false));
        let provider = ProviderEntry::new(
            std::any::TypeId::of::<String>(),
            type_name::<String>(),
            ProviderLifetime::Singleton,
            Arc::new({
                let failed_once = Arc::clone(&failed_once);
                move |_container| {
                    if failed_once.swap(true, Ordering::SeqCst) {
                        Ok(Arc::new("recovered".to_owned()) as Arc<dyn Any + Send + Sync>)
                    } else {
                        Err(crate::NidusError::MissingProvider {
                            type_name: "transient failure",
                        })
                    }
                }
            }),
        );
        let container = Container::new();

        assert!(provider.resolve_erased(&container).is_err());
        let value = provider
            .resolve_erased(&container)
            .unwrap()
            .downcast::<String>()
            .unwrap();
        assert_eq!(&*value, "recovered");
    }

    #[test]
    fn singleton_provider_recovers_from_poisoned_cache() {
        let provider = Arc::new(ProviderEntry::new(
            std::any::TypeId::of::<String>(),
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

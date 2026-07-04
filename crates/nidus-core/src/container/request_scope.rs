use std::{
    any::{Any, TypeId, type_name},
    sync::{Arc, Condvar, Mutex, MutexGuard},
};

use crate::{
    Container, Inject, NidusError, Optional, ProviderLifetime, Result, Scoped, resolution,
};

use super::{TypeIdMap, downcast};

/// Per-request dependency scope.
pub struct RequestScope<'a> {
    container: RequestScopeContainer<'a>,
    request_instances: Mutex<TypeIdMap<RequestInstanceState>>,
    request_instance_ready: Condvar,
}

enum RequestInstanceState {
    Initializing,
    Ready(Arc<dyn Any + Send + Sync>),
}

/// Shared request scope handle suitable for HTTP request extensions.
pub type SharedRequestScope = Arc<RequestScope<'static>>;

enum RequestScopeContainer<'a> {
    Borrowed(&'a Container),
    Shared(Arc<Container>),
}

impl RequestScopeContainer<'_> {
    fn as_ref(&self) -> &Container {
        match self {
            Self::Borrowed(container) => container,
            Self::Shared(container) => container,
        }
    }
}

impl<'a> RequestScope<'a> {
    pub(super) fn borrowed(container: &'a Container) -> Self {
        Self {
            container: RequestScopeContainer::Borrowed(container),
            request_instances: Mutex::new(TypeIdMap::default()),
            request_instance_ready: Condvar::new(),
        }
    }
}

impl RequestScope<'_> {
    pub(crate) fn container(&self) -> &Container {
        self.container.as_ref()
    }

    /// Resolves a dependency in this request scope.
    pub fn resolve<T>(&self) -> Result<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let entry = self.container().entry::<T>()?;
        let erased = match entry.lifetime() {
            ProviderLifetime::Request => {
                let type_id = TypeId::of::<T>();
                self.resolve_request_instance(type_id, type_name::<T>(), || {
                    entry.resolve_erased_in_scope(self)
                })?
            }
            ProviderLifetime::Singleton | ProviderLifetime::Transient => {
                entry.resolve_erased(self.container())?
            }
        };

        downcast::<T>(erased)
    }

    /// Resolves a typed dependency reference in this request scope.
    pub fn inject<T>(&self) -> Result<Inject<T>>
    where
        T: Send + Sync + 'static,
    {
        self.resolve::<T>().map(Inject::new)
    }

    /// Resolves an optional typed dependency reference in this request scope.
    ///
    /// Missing providers become `Optional::new(None)`, while registered providers
    /// that fail to construct still return their original error.
    pub fn optional<T>(&self) -> Result<Optional<T>>
    where
        T: Send + Sync + 'static,
    {
        match self.inject::<T>() {
            Ok(value) => Ok(Optional::new(Some(value))),
            Err(NidusError::MissingProvider { .. }) => Ok(Optional::new(None)),
            Err(error) => Err(error),
        }
    }

    /// Resolves a request-scoped dependency wrapper in this request scope.
    pub fn scoped<T>(&self) -> Result<Scoped<T>>
    where
        T: Send + Sync + 'static,
    {
        self.inject::<T>().map(Scoped::new)
    }

    fn resolve_request_instance(
        &self,
        type_id: TypeId,
        type_name: &'static str,
        create: impl FnOnce() -> Result<Arc<dyn Any + Send + Sync>>,
    ) -> Result<Arc<dyn Any + Send + Sync>> {
        let mut create = Some(create);
        loop {
            let mut instances = lock_unpoisoned(&self.request_instances);
            match instances.get(&type_id) {
                Some(RequestInstanceState::Ready(instance)) => return Ok(Arc::clone(instance)),
                Some(RequestInstanceState::Initializing) => {
                    if resolution::is_active(type_id) {
                        return Err(NidusError::CircularProviderResolution { type_name });
                    }
                    drop(wait_unpoisoned(&self.request_instance_ready, instances));
                }
                None => {
                    let _guard = resolution::enter(type_id, type_name)?;
                    instances.insert(type_id, RequestInstanceState::Initializing);
                    drop(instances);

                    let initializer = create
                        .take()
                        .expect("request instance factory can only be used by initializer");
                    let instance = initializer();
                    let mut instances = lock_unpoisoned(&self.request_instances);
                    match instance {
                        Ok(instance) => {
                            instances.insert(
                                type_id,
                                RequestInstanceState::Ready(Arc::clone(&instance)),
                            );
                            self.request_instance_ready.notify_all();
                            return Ok(instance);
                        }
                        Err(error) => {
                            instances.remove(&type_id);
                            self.request_instance_ready.notify_all();
                            return Err(error);
                        }
                    }
                }
            }
        }
    }
}

impl RequestScope<'static> {
    /// Creates a request scope that owns a shared container handle.
    pub fn from_shared_container(container: Arc<Container>) -> Self {
        Self {
            container: RequestScopeContainer::Shared(container),
            request_instances: Mutex::new(TypeIdMap::default()),
            request_instance_ready: Condvar::new(),
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
    use std::{sync::Arc, thread};

    use super::RequestScope;
    use crate::Container;

    #[derive(Debug, Eq, PartialEq)]
    struct RequestValue(u64);

    #[test]
    fn request_scope_recovers_from_poisoned_instance_cache() {
        let mut container = Container::new();
        container
            .register_request_scoped::<RequestValue, _>(|_scope| Ok(RequestValue(42)))
            .unwrap();
        let scope = Arc::new(RequestScope::from_shared_container(Arc::new(container)));
        let poisoned_scope = Arc::clone(&scope);

        let panic = thread::spawn(move || {
            let _instances = poisoned_scope.request_instances.lock().unwrap();
            panic!("poison request scope cache");
        });
        assert!(panic.join().is_err());

        assert_eq!(*scope.resolve::<RequestValue>().unwrap(), RequestValue(42));
    }
}

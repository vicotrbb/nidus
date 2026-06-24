use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use crate::{Container, Inject, NidusError, Optional, ProviderLifetime, Result, Scoped};

use super::downcast;

/// Per-request dependency scope.
pub struct RequestScope<'a> {
    container: RequestScopeContainer<'a>,
    request_instances: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
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
            request_instances: Mutex::new(HashMap::new()),
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
                if let Some(existing) = lock_unpoisoned(&self.request_instances)
                    .get(&type_id)
                    .cloned()
                {
                    existing
                } else {
                    let instance = entry.resolve_erased_in_scope(self)?;
                    lock_unpoisoned(&self.request_instances).insert(type_id, Arc::clone(&instance));
                    instance
                }
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
}

impl RequestScope<'static> {
    /// Creates a request scope that owns a shared container handle.
    pub fn from_shared_container(container: Arc<Container>) -> Self {
        Self {
            container: RequestScopeContainer::Shared(container),
            request_instances: Mutex::new(HashMap::new()),
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

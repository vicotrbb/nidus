//! Typed dependency container primitives.

mod dependency;

use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{NidusError, ProviderEntry, ProviderLifetime, Result};

pub use dependency::{Factory, Inject, Lazy, Optional, Scoped};

/// Type-indexed dependency container.
#[derive(Default)]
pub struct Container {
    providers: HashMap<TypeId, ProviderEntry>,
}

impl Container {
    /// Creates an empty container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a request scope for request-lifetime providers.
    pub fn request_scope(&self) -> RequestScope<'_> {
        RequestScope {
            container: RequestScopeContainer::Borrowed(self),
            request_instances: Mutex::new(HashMap::new()),
        }
    }

    /// Registers a concrete singleton value.
    pub fn register_singleton<T>(&mut self, value: T) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        let value = Arc::new(value);
        self.insert::<T>(ProviderLifetime::Singleton, move |_container| {
            Ok(Arc::clone(&value) as Arc<dyn Any + Send + Sync>)
        })
    }

    /// Replaces a singleton provider, intended for explicit test overrides.
    pub fn override_singleton<T>(&mut self, value: T) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        self.providers.remove(&TypeId::of::<T>());
        self.register_singleton(value)
    }

    /// Registers a provider factory.
    pub fn register_factory<T, F>(&mut self, lifetime: ProviderLifetime, factory: F) -> Result<()>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.insert::<T>(lifetime, move |container| {
            factory(container).map(|value| Arc::new(value) as Arc<dyn Any + Send + Sync>)
        })
    }

    /// Registers a singleton provider factory.
    pub fn register_singleton_factory<T, F>(&mut self, factory: F) -> Result<()>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.register_factory::<T, F>(ProviderLifetime::Singleton, factory)
    }

    /// Registers a transient provider factory.
    pub fn register_transient<T, F>(&mut self, factory: F) -> Result<()>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.register_factory::<T, F>(ProviderLifetime::Transient, factory)
    }

    /// Registers a request-lifetime provider factory.
    pub fn register_request<T, F>(&mut self, factory: F) -> Result<()>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.register_factory::<T, F>(ProviderLifetime::Request, factory)
    }

    /// Registers a request-lifetime provider factory that resolves dependencies
    /// through the active request scope.
    pub fn register_request_scoped<T, F>(&mut self, factory: F) -> Result<()>
    where
        T: Send + Sync + 'static,
        F: for<'scope> Fn(&RequestScope<'scope>) -> Result<T> + Send + Sync + 'static,
    {
        self.insert_request_scoped::<T>(
            |_container| {
                Err(NidusError::RequestScopeRequired {
                    type_name: type_name::<T>(),
                })
            },
            move |scope| factory(scope).map(|value| Arc::new(value) as Arc<dyn Any + Send + Sync>),
        )
    }

    /// Resolves a typed dependency reference.
    pub fn inject<T>(&self) -> Result<Inject<T>>
    where
        T: Send + Sync + 'static,
    {
        self.resolve::<T>().map(Inject::new)
    }

    /// Resolves an optional typed dependency reference.
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

    /// Resolves a shared typed dependency.
    pub fn resolve<T>(&self) -> Result<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let entry = self.entry::<T>()?;
        if entry.lifetime() == ProviderLifetime::Request {
            return Err(NidusError::RequestScopeRequired {
                type_name: type_name::<T>(),
            });
        }
        let erased = entry.resolve_erased(self)?;
        downcast::<T>(erased)
    }

    fn insert<T>(
        &mut self,
        lifetime: ProviderLifetime,
        factory: impl Fn(&Container) -> Result<Arc<dyn Any + Send + Sync>> + Send + Sync + 'static,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        if self.providers.contains_key(&type_id) {
            return Err(NidusError::DuplicateProvider {
                type_name: type_name::<T>(),
            });
        }

        self.providers.insert(
            type_id,
            ProviderEntry::new(type_name::<T>(), lifetime, Arc::new(factory)),
        );
        Ok(())
    }

    fn insert_request_scoped<T>(
        &mut self,
        factory: impl Fn(&Container) -> Result<Arc<dyn Any + Send + Sync>> + Send + Sync + 'static,
        request_factory: impl for<'scope> Fn(
            &RequestScope<'scope>,
        ) -> Result<Arc<dyn Any + Send + Sync>>
        + Send
        + Sync
        + 'static,
    ) -> Result<()>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        if self.providers.contains_key(&type_id) {
            return Err(NidusError::DuplicateProvider {
                type_name: type_name::<T>(),
            });
        }

        self.providers.insert(
            type_id,
            ProviderEntry::new_request_scoped(
                type_name::<T>(),
                Arc::new(factory),
                Arc::new(request_factory),
            ),
        );
        Ok(())
    }

    fn entry<T>(&self) -> Result<&ProviderEntry>
    where
        T: Send + Sync + 'static,
    {
        self.providers
            .get(&TypeId::of::<T>())
            .ok_or_else(|| NidusError::MissingProvider {
                type_name: type_name::<T>(),
            })
    }
}

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
                if let Some(existing) = self
                    .request_instances
                    .lock()
                    .expect("request scope mutex poisoned")
                    .get(&type_id)
                    .cloned()
                {
                    existing
                } else {
                    let instance = entry.resolve_erased_in_scope(self)?;
                    self.request_instances
                        .lock()
                        .expect("request scope mutex poisoned")
                        .insert(type_id, Arc::clone(&instance));
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

fn downcast<T>(erased: Arc<dyn Any + Send + Sync>) -> Result<Arc<T>>
where
    T: Send + Sync + 'static,
{
    erased
        .downcast::<T>()
        .map_err(|_| NidusError::MissingProvider {
            type_name: type_name::<T>(),
        })
}

//! Typed dependency container primitives.

use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    ops::Deref,
    sync::Arc,
};

use crate::{NidusError, ProviderEntry, ProviderLifetime, Result};

/// A typed dependency reference resolved from the container.
#[derive(Debug)]
pub struct Inject<T: Send + Sync + 'static>(Arc<T>);

impl<T: Send + Sync + 'static> Inject<T> {
    /// Wraps an already constructed shared dependency.
    pub fn new(value: Arc<T>) -> Self {
        Self(value)
    }

    /// Returns a cloned shared pointer to the dependency.
    pub fn into_inner(self) -> Arc<T> {
        self.0
    }
}

impl<T: Send + Sync + 'static> Clone for Inject<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: Send + Sync + 'static> Deref for Inject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

/// Optional dependency wrapper.
#[derive(Clone, Debug)]
pub struct Optional<T: Send + Sync + 'static>(Option<Inject<T>>);

impl<T: Send + Sync + 'static> Optional<T> {
    /// Creates an optional dependency wrapper.
    pub fn new(value: Option<Inject<T>>) -> Self {
        Self(value)
    }

    /// Returns the optional dependency.
    pub fn into_option(self) -> Option<Inject<T>> {
        self.0
    }
}

/// Lazily resolved dependency wrapper.
#[derive(Clone)]
pub struct Lazy<T: Send + Sync + 'static> {
    resolver: Arc<dyn Fn() -> Result<Inject<T>> + Send + Sync>,
}

impl<T: Send + Sync + 'static> Lazy<T> {
    /// Creates a lazy dependency from a resolver closure.
    pub fn new(resolver: impl Fn() -> Result<Inject<T>> + Send + Sync + 'static) -> Self {
        Self {
            resolver: Arc::new(resolver),
        }
    }

    /// Resolves the dependency.
    pub fn get(&self) -> Result<Inject<T>> {
        (self.resolver)()
    }
}

/// Factory dependency wrapper.
#[derive(Clone)]
pub struct Factory<T: Send + Sync + 'static> {
    factory: Arc<dyn Fn() -> Result<T> + Send + Sync>,
}

impl<T: Send + Sync + 'static> Factory<T> {
    /// Creates a typed factory wrapper.
    pub fn new(factory: impl Fn() -> Result<T> + Send + Sync + 'static) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }

    /// Creates a fresh value.
    pub fn create(&self) -> Result<T> {
        (self.factory)()
    }
}

/// Request-scoped dependency wrapper.
#[derive(Clone, Debug)]
pub struct Scoped<T: Send + Sync + 'static>(Inject<T>);

impl<T: Send + Sync + 'static> Scoped<T> {
    /// Creates a scoped dependency wrapper.
    pub fn new(value: Inject<T>) -> Self {
        Self(value)
    }

    /// Returns the scoped dependency.
    pub fn into_inject(self) -> Inject<T> {
        self.0
    }
}

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

    /// Resolves a typed dependency reference.
    pub fn inject<T>(&self) -> Result<Inject<T>>
    where
        T: Send + Sync + 'static,
    {
        self.resolve::<T>().map(Inject::new)
    }

    /// Resolves a shared typed dependency.
    pub fn resolve<T>(&self) -> Result<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let entry =
            self.providers
                .get(&TypeId::of::<T>())
                .ok_or_else(|| NidusError::MissingProvider {
                    type_name: type_name::<T>(),
                })?;
        let erased = entry.resolve_erased(self)?;
        erased
            .downcast::<T>()
            .map_err(|_| NidusError::MissingProvider {
                type_name: type_name::<T>(),
            })
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
}

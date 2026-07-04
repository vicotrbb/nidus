//! Typed dependency container primitives.

mod dependency;
mod request_scope;

use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::Arc,
};

use crate::{NidusError, ProviderEntry, ProviderLifetime, Result};

pub use dependency::{Factory, Inject, Lazy, Optional, Scoped};
pub use request_scope::{RequestScope, SharedRequestScope};

/// Hash map keyed by `TypeId` using the identity hasher below.
pub(crate) type TypeIdMap<V> = HashMap<TypeId, V, BuildHasherDefault<TypeIdHasher>>;

/// Identity hasher for `TypeId` keys.
///
/// `TypeId`'s `Hash` impl feeds an already high-quality compiler-generated
/// 64-bit hash through `write_u64`, so mixing it again through the default
/// SipHash only adds cost. Store that value directly, mirroring the map used
/// by `http::Extensions`.
#[derive(Default)]
pub(crate) struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    fn write(&mut self, bytes: &[u8]) {
        // TypeId only calls write_u64; keep a correct fallback for any other
        // key shape rather than assuming the std implementation never changes.
        for &byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(byte);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Type-indexed dependency container.
#[derive(Default)]
pub struct Container {
    providers: TypeIdMap<ProviderEntry>,
}

impl Container {
    /// Creates an empty container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a request scope for request-lifetime providers.
    pub fn request_scope(&self) -> RequestScope<'_> {
        RequestScope::borrowed(self)
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

    /// Eagerly constructs every registered singleton provider and caches it.
    ///
    /// Singletons are otherwise constructed lazily on first resolution, which
    /// uses a blocking `Condvar` wait when two callers race to construct the
    /// same provider. Calling this at startup pre-constructs each singleton so
    /// later resolutions (including from async request handlers) hit the cached
    /// value and never reach that wait, avoiding an async-runtime worker
    /// stalling on first use. Transient and request-lifetime providers are
    /// skipped.
    ///
    /// A singleton whose factory errors or panics will do so here, failing
    /// startup fast instead of on first request.
    pub fn eagerly_resolve_singletons(&self) -> Result<()> {
        for entry in self.providers.values() {
            if entry.lifetime() == ProviderLifetime::Singleton {
                entry.resolve_erased(self)?;
            }
        }
        Ok(())
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
            ProviderEntry::new(type_id, type_name::<T>(), lifetime, Arc::new(factory)),
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
                type_id,
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

#[cfg(test)]
mod tests {
    use std::{
        any::TypeId,
        hash::{Hash, Hasher},
    };

    use super::TypeIdHasher;

    #[test]
    fn type_id_hasher_uses_written_u64_directly() {
        let mut hasher = TypeIdHasher::default();
        hasher.write_u64(42);
        assert_eq!(hasher.finish(), 42);
    }

    #[test]
    fn type_id_hasher_distinguishes_type_ids() {
        let mut first = TypeIdHasher::default();
        TypeId::of::<u32>().hash(&mut first);
        let mut second = TypeIdHasher::default();
        TypeId::of::<u64>().hash(&mut second);
        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn type_id_hasher_byte_fallback_mixes_all_bytes() {
        let mut first = TypeIdHasher::default();
        first.write(b"ab");
        let mut second = TypeIdHasher::default();
        second.write(b"ba");
        assert_ne!(first.finish(), second.finish());
    }
}

use std::{ops::Deref, sync::Arc};

use crate::Result;

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

    /// Returns `true` when the optional dependency is present.
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    /// Returns `true` when the optional dependency is absent.
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }

    /// Returns a shared reference to the optional dependency.
    pub fn as_ref(&self) -> Option<&Inject<T>> {
        self.0.as_ref()
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

impl<T: Send + Sync + 'static> Deref for Scoped<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

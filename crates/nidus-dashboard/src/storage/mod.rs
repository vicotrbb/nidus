//! Dashboard storage backends.

mod memory;

use std::{future::Future, pin::Pin};

use crate::{DashboardOperation, error::Result};

pub use memory::MemoryDashboardStorage;

/// Boxed storage future.
pub type StorageFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Dashboard storage backend.
pub trait DashboardStorageBackend: Clone + Send + Sync + 'static {
    /// Records a timeline operation.
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()>;

    /// Lists newest operations first.
    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>>;

    /// Prunes to the newest `max_events` operations.
    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()>;
}

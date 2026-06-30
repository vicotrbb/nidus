//! Dashboard storage backends.

mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;

use std::{future::Future, pin::Pin};

use crate::{DashboardOperation, DashboardRouteSnapshot, error::Result};

pub use memory::MemoryDashboardStorage;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteDashboardStorage;

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

    /// Records a route snapshot.
    fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> StorageFuture<'_, ()>;

    /// Lists route snapshots.
    fn list_route_snapshots(&self) -> StorageFuture<'_, Vec<DashboardRouteSnapshot>>;
}

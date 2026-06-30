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

/// Runtime dashboard storage backend selected from configuration.
#[derive(Clone, Debug)]
pub enum DashboardStorageHandle {
    /// In-memory storage.
    Memory(MemoryDashboardStorage),
    /// SQLite storage.
    #[cfg(feature = "sqlite")]
    Sqlite(SqliteDashboardStorage),
}

impl DashboardStorageHandle {
    /// Creates an in-memory storage handle.
    pub fn memory() -> Self {
        Self::Memory(MemoryDashboardStorage::new())
    }

    /// Returns the runtime storage mode.
    pub fn mode_name(&self) -> &'static str {
        match self {
            Self::Memory(_) => "memory",
            #[cfg(feature = "sqlite")]
            Self::Sqlite(_) => "sqlite",
        }
    }
}

impl DashboardStorageBackend for DashboardStorageHandle {
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()> {
        match self {
            Self::Memory(storage) => storage.record_operation(operation),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(storage) => storage.record_operation(operation),
        }
    }

    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>> {
        match self {
            Self::Memory(storage) => storage.list_operations(limit),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(storage) => storage.list_operations(limit),
        }
    }

    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()> {
        match self {
            Self::Memory(storage) => storage.prune(max_events),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(storage) => storage.prune(max_events),
        }
    }

    fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> StorageFuture<'_, ()> {
        match self {
            Self::Memory(storage) => storage.record_route_snapshot(route),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(storage) => storage.record_route_snapshot(route),
        }
    }

    fn list_route_snapshots(&self) -> StorageFuture<'_, Vec<DashboardRouteSnapshot>> {
        match self {
            Self::Memory(storage) => storage.list_route_snapshots(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(storage) => storage.list_route_snapshots(),
        }
    }
}

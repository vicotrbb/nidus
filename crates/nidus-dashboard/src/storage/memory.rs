use std::sync::{Arc, Mutex};

use crate::{DashboardOperation, DashboardRouteSnapshot};

use super::{DashboardStorageBackend, StorageFuture};

/// In-memory dashboard storage.
#[derive(Clone, Debug, Default)]
pub struct MemoryDashboardStorage {
    operations: Arc<Mutex<Vec<DashboardOperation>>>,
    routes: Arc<Mutex<Vec<DashboardRouteSnapshot>>>,
}

impl MemoryDashboardStorage {
    /// Creates empty memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

impl DashboardStorageBackend for MemoryDashboardStorage {
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            self.operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(operation);
            Ok(())
        })
    }

    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>> {
        Box::pin(async move {
            let operations = self
                .operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(operations.iter().rev().take(limit).cloned().collect())
        })
    }

    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            let mut operations = self
                .operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let len = operations.len();
            if len > max_events {
                operations.drain(..len - max_events);
            }
            Ok(())
        })
    }

    fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            self.routes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(route);
            Ok(())
        })
    }

    fn list_route_snapshots(&self) -> StorageFuture<'_, Vec<DashboardRouteSnapshot>> {
        Box::pin(async move {
            Ok(self
                .routes
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone())
        })
    }
}

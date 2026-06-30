//! Dashboard capture hooks.

use std::collections::BTreeMap;

use crate::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus,
    error::Result,
    storage::{DashboardStorageBackend, MemoryDashboardStorage},
};

/// Dashboard collector.
#[derive(Clone, Debug)]
pub struct DashboardCollector<S = MemoryDashboardStorage>
where
    S: DashboardStorageBackend,
{
    storage: S,
}

impl<S> DashboardCollector<S>
where
    S: DashboardStorageBackend,
{
    /// Creates a collector.
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    /// Records an observed event publication.
    pub async fn record_event<I, K, V>(
        &self,
        name: impl Into<String>,
        operation_id: Option<&str>,
        attributes: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let operation = DashboardOperation {
            id: operation_id
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            kind: DashboardOperationKind::Event,
            name: name.into(),
            status: DashboardOperationStatus::Success,
            timestamp_ms: now_ms(),
            duration_ms: None,
            correlation_id: operation_id.map(str::to_owned),
            attributes: attributes
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect::<BTreeMap<_, _>>(),
            payload: None,
        };
        self.storage.record_operation(operation).await
    }
}

fn now_ms() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}

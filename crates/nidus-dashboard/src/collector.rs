//! Dashboard capture hooks.

use std::collections::BTreeMap;

use crate::{
    DashboardCapture,
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
    capture: DashboardCapture,
}

impl<S> DashboardCollector<S>
where
    S: DashboardStorageBackend,
{
    /// Creates a collector.
    pub fn new(storage: S, capture: DashboardCapture) -> Self {
        Self { storage, capture }
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

    /// Records an observed event publication with an optional redacted payload.
    pub async fn record_payload_event(
        &self,
        name: impl Into<String>,
        operation_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let payload = if self.capture.captures_payloads() {
            Some(redact_value(payload, self.capture.redacted_fields()))
        } else {
            None
        };
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
            attributes: BTreeMap::new(),
            payload,
        };
        self.storage.record_operation(operation).await
    }
}

fn now_ms() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}

fn redact_value(mut value: serde_json::Value, redacted_fields: &[String]) -> serde_json::Value {
    match &mut value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if redacted_fields
                    .iter()
                    .any(|field| field.eq_ignore_ascii_case(key))
                {
                    *value = serde_json::Value::String("[redacted]".to_owned());
                } else {
                    *value = redact_value(std::mem::take(value), redacted_fields);
                }
            }
            value
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                *item = redact_value(std::mem::take(item), redacted_fields);
            }
            value
        }
        _ => value,
    }
}

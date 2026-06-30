//! Dashboard capture hooks.

use std::collections::BTreeMap;

use crate::{
    DashboardCapture, DashboardOperation, DashboardOperationKind, DashboardOperationStatus,
    DashboardRetention,
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
    retention: DashboardRetention,
}

impl<S> DashboardCollector<S>
where
    S: DashboardStorageBackend,
{
    /// Creates a collector.
    pub fn new(storage: S, capture: DashboardCapture) -> Self {
        Self::with_retention(storage, capture, DashboardRetention::default())
    }

    /// Creates a collector with explicit retention.
    pub fn with_retention(
        storage: S,
        capture: DashboardCapture,
        retention: DashboardRetention,
    ) -> Self {
        Self {
            storage,
            capture,
            retention,
        }
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
        self.record_and_prune(operation).await
    }

    /// Records an observed event publication with an optional redacted payload.
    pub async fn record_payload_event(
        &self,
        name: impl Into<String>,
        operation_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let payload = if self.capture.captures_payloads() {
            Some(cap_payload(
                redact_value(payload, self.capture.redacted_fields()),
                self.capture.payload_byte_cap(),
            )?)
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
        self.record_and_prune(operation).await
    }

    /// Records an observed job run.
    pub async fn record_job(
        &self,
        name: impl Into<String>,
        run_id: Option<&str>,
        success: bool,
        duration_ms: u64,
    ) -> Result<()> {
        let operation = DashboardOperation {
            id: run_id
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            kind: DashboardOperationKind::Job,
            name: name.into(),
            status: if success {
                DashboardOperationStatus::Success
            } else {
                DashboardOperationStatus::Failure
            },
            timestamp_ms: now_ms(),
            duration_ms: Some(duration_ms),
            correlation_id: run_id.map(str::to_owned),
            attributes: BTreeMap::new(),
            payload: None,
        };
        self.record_and_prune(operation).await
    }

    async fn record_and_prune(&self, operation: DashboardOperation) -> Result<()> {
        self.storage.record_operation(operation).await?;
        self.storage.prune(self.retention.max_event_count()).await
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

fn cap_payload(value: serde_json::Value, max_bytes: usize) -> Result<serde_json::Value> {
    if serde_json::to_vec(&value)?.len() <= max_bytes {
        return Ok(value);
    }

    Ok(serde_json::json!({
        "truncated": true,
        "max_payload_bytes": max_bytes,
    }))
}

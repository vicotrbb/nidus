use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus, DashboardRouteSnapshot,
    error::{DashboardError, Result},
};

use super::{DashboardStorageBackend, StorageFuture};

/// SQLite dashboard storage.
#[derive(Clone, Debug)]
pub struct SqliteDashboardStorage {
    pool: SqlitePool,
}

impl SqliteDashboardStorage {
    /// Connects to SQLite and runs dashboard migrations.
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        let _ = sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS dashboard_operations (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                duration_ms INTEGER,
                correlation_id TEXT,
                attributes_json TEXT NOT NULL,
                payload_json TEXT
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS dashboard_operations_timestamp_idx
             ON dashboard_operations(timestamp_ms)",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS dashboard_routes (
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                summary TEXT,
                guards_json TEXT NOT NULL,
                pipes_json TEXT NOT NULL,
                validates INTEGER NOT NULL,
                PRIMARY KEY (method, path)
            )",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    /// Connects lazily to SQLite.
    pub fn connect_lazy(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_lazy(database_url)?;
        Ok(Self { pool })
    }

    async fn migrate(&self) -> Result<()> {
        let _ = sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&self.pool)
            .await;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS dashboard_operations (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                duration_ms INTEGER,
                correlation_id TEXT,
                attributes_json TEXT NOT NULL,
                payload_json TEXT
            )",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS dashboard_operations_timestamp_idx
             ON dashboard_operations(timestamp_ms)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS dashboard_routes (
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                summary TEXT,
                guards_json TEXT NOT NULL,
                pipes_json TEXT NOT NULL,
                validates INTEGER NOT NULL,
                PRIMARY KEY (method, path)
            )",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

impl DashboardStorageBackend for SqliteDashboardStorage {
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            self.migrate().await?;
            let attributes_json = serde_json::to_string(&operation.attributes)?;
            let payload_json = operation
                .payload
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            sqlx::query(
                "INSERT OR REPLACE INTO dashboard_operations
                 (id, kind, name, status, timestamp_ms, duration_ms, correlation_id, attributes_json, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(&operation.id)
            .bind(kind_to_str(&operation.kind))
            .bind(&operation.name)
            .bind(status_to_str(&operation.status))
            .bind(operation.timestamp_ms)
            .bind(operation.duration_ms.map(|value| value as i64))
            .bind(&operation.correlation_id)
            .bind(attributes_json)
            .bind(payload_json)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>> {
        Box::pin(async move {
            self.migrate().await?;
            let rows = sqlx::query(
                "SELECT id, kind, name, status, timestamp_ms, duration_ms, correlation_id, attributes_json, payload_json
                 FROM dashboard_operations
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ?1",
            )
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

            rows.into_iter().map(row_to_operation).collect()
        })
    }

    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            self.migrate().await?;
            sqlx::query(
                "DELETE FROM dashboard_operations
                 WHERE id NOT IN (
                     SELECT id FROM dashboard_operations
                     ORDER BY timestamp_ms DESC, id DESC
                     LIMIT ?1
                 )",
            )
            .bind(max_events as i64)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            self.migrate().await?;
            sqlx::query(
                "INSERT OR REPLACE INTO dashboard_routes
                 (method, path, summary, guards_json, pipes_json, validates)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(&route.method)
            .bind(&route.path)
            .bind(&route.summary)
            .bind(serde_json::to_string(&route.guards)?)
            .bind(serde_json::to_string(&route.pipes)?)
            .bind(i64::from(route.validates))
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn list_route_snapshots(&self) -> StorageFuture<'_, Vec<DashboardRouteSnapshot>> {
        Box::pin(async move {
            self.migrate().await?;
            let rows = sqlx::query(
                "SELECT method, path, summary, guards_json, pipes_json, validates
                 FROM dashboard_routes
                 ORDER BY path ASC, method ASC",
            )
            .fetch_all(&self.pool)
            .await?;

            rows.into_iter().map(row_to_route).collect()
        })
    }
}

fn row_to_operation(row: sqlx::sqlite::SqliteRow) -> Result<DashboardOperation> {
    let kind: String = row.try_get("kind")?;
    let status: String = row.try_get("status")?;
    let duration_ms: Option<i64> = row.try_get("duration_ms")?;
    let attributes_json: String = row.try_get("attributes_json")?;
    let payload_json: Option<String> = row.try_get("payload_json")?;

    Ok(DashboardOperation {
        id: row.try_get("id")?,
        kind: parse_kind(&kind)?,
        name: row.try_get("name")?,
        status: parse_status(&status)?,
        timestamp_ms: row.try_get("timestamp_ms")?,
        duration_ms: duration_ms.map(|value| value as u64),
        correlation_id: row.try_get("correlation_id")?,
        attributes: serde_json::from_str(&attributes_json)?,
        payload: payload_json
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
    })
}

fn row_to_route(row: sqlx::sqlite::SqliteRow) -> Result<DashboardRouteSnapshot> {
    let guards_json: String = row.try_get("guards_json")?;
    let pipes_json: String = row.try_get("pipes_json")?;
    let validates: i64 = row.try_get("validates")?;

    Ok(DashboardRouteSnapshot {
        method: row.try_get("method")?,
        path: row.try_get("path")?,
        summary: row.try_get("summary")?,
        guards: serde_json::from_str(&guards_json)?,
        pipes: serde_json::from_str(&pipes_json)?,
        validates: validates != 0,
    })
}

fn kind_to_str(kind: &DashboardOperationKind) -> &'static str {
    match kind {
        DashboardOperationKind::Http => "http",
        DashboardOperationKind::Event => "event",
        DashboardOperationKind::Job => "job",
        DashboardOperationKind::Lifecycle => "lifecycle",
        DashboardOperationKind::Adapter => "adapter",
    }
}

fn status_to_str(status: &DashboardOperationStatus) -> &'static str {
    match status {
        DashboardOperationStatus::Success => "success",
        DashboardOperationStatus::Failure => "failure",
        DashboardOperationStatus::Running => "running",
    }
}

fn parse_kind(value: &str) -> Result<DashboardOperationKind> {
    match value {
        "http" => Ok(DashboardOperationKind::Http),
        "event" => Ok(DashboardOperationKind::Event),
        "job" => Ok(DashboardOperationKind::Job),
        "lifecycle" => Ok(DashboardOperationKind::Lifecycle),
        "adapter" => Ok(DashboardOperationKind::Adapter),
        other => Err(DashboardError::Storage(format!(
            "unknown operation kind `{other}`"
        ))),
    }
}

fn parse_status(value: &str) -> Result<DashboardOperationStatus> {
    match value {
        "success" => Ok(DashboardOperationStatus::Success),
        "failure" => Ok(DashboardOperationStatus::Failure),
        "running" => Ok(DashboardOperationStatus::Running),
        other => Err(DashboardError::Storage(format!(
            "unknown operation status `{other}`"
        ))),
    }
}

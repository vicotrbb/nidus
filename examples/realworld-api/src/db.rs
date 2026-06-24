use nidus::prelude::{Container, HttpError, NidusError, ProviderRegistrant, Result};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::{future::Future, pin::Pin};

use crate::config::AppConfig;

#[derive(Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl ProviderRegistrant for Database {
    fn register_provider(_container: &mut Container) -> Result<()> {
        Ok(())
    }
}

pub fn initialize_database(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        let config = container.resolve::<AppConfig>()?;
        let database = Database::connect(&config.database_url)
            .await
            .map_err(|error| NidusError::ApplicationBuild {
                message: format!("database initialization failed: {error}"),
            })?;
        container.register_singleton(database)
    })
}

impl Database {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(database_url)
            .await?;

        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await?;
        initialize_schema(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

pub fn map_db_error(error: sqlx::Error) -> HttpError {
    tracing::error!(error = %error, "database operation failed");
    HttpError::internal_server_error()
}

async fn initialize_schema(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            email TEXT NOT NULL UNIQUE,
            display_name TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            owner_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            FOREIGN KEY(owner_id) REFERENCES users(id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            completed INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(project_id) REFERENCES projects(id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

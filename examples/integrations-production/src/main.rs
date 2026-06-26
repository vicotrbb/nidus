use std::sync::Arc;

use nidus::prelude::{
    Application, Config, Container, HealthRegistry, Module, ModuleBuilder, ModuleDefinition,
    ModuleGraph, NidusError,
};
use nidus_cache::{CacheConfig, MokaCacheProvider};
use nidus_sqlx::SqlitePoolProvider;

#[derive(Clone, Debug)]
struct AppConfig {
    database_url: String,
    cache_namespace: String,
}

struct AppModule;

impl Module for AppModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("AppModule")
            .provider_typed::<SqlitePoolProvider>()
            .provider_typed::<MokaCacheProvider>()
            .export_typed::<SqlitePoolProvider>()
            .export_typed::<MokaCacheProvider>()
            .build()
    }
}

async fn build_app(config: AppConfig) -> nidus::prelude::Result<Application> {
    let mut container = Container::new();
    SqlitePoolProvider::builder()
        .database_url(config.database_url)
        .max_connections(1)
        .register(&mut container)
        .await
        .map_err(|error| NidusError::ApplicationBuild {
            message: error.to_string(),
        })?;
    MokaCacheProvider::builder()
        .config(
            CacheConfig::new()
                .namespace(config.cache_namespace)
                .max_capacity(10_000),
        )
        .register(&mut container)
        .map_err(|error| NidusError::ApplicationBuild {
            message: error.to_string(),
        })?;

    let graph = ModuleGraph::from_root::<AppModule>()?;
    Ok(Application::new(container, graph))
}

fn health_registry(container: &Container) -> nidus::prelude::Result<HealthRegistry> {
    let database = container.resolve::<SqlitePoolProvider>()?;
    let cache = container.resolve::<MokaCacheProvider>()?;
    Ok(HealthRegistry::new()
        .ready_check("database", move || {
            let database = Arc::clone(&database);
            async move { database.health_status().await }
        })
        .ready_check_sync("cache", move || cache.health_status()))
}

fn config_from_nidus_config(config: Config) -> nidus::prelude::Result<AppConfig> {
    Ok(AppConfig {
        database_url: config
            .get_required_path_typed(["database", "url"])
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })?,
        cache_namespace: config
            .get_required_path_typed(["cache", "namespace"])
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })?,
    })
}

#[cfg(test)]
fn test_config() -> AppConfig {
    AppConfig {
        database_url: "sqlite::memory:".to_owned(),
        cache_namespace: "users".to_owned(),
    }
}

#[tokio::main]
async fn main() -> nidus::prelude::Result<()> {
    let config = Config::from_env_prefix("APP");
    let app = build_app(config_from_nidus_config(config)?).await?;
    let _health = health_registry(app.container())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn example_wires_production_integrations() {
        let app = build_app(test_config()).await.unwrap();
        assert!(app.container().resolve::<SqlitePoolProvider>().is_ok());
        assert!(app.container().resolve::<MokaCacheProvider>().is_ok());
        assert!(health_registry(app.container()).is_ok());
    }
}

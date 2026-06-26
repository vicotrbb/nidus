use nidus_core::{Container, ModuleBuilder};
use nidus_sqlx::{SqlitePoolConfig, SqlitePoolProvider};

#[tokio::test]
async fn sqlite_provider_registers_real_pool_in_container() {
    let mut container = Container::new();

    SqlitePoolProvider::builder()
        .database_url("sqlite::memory:")
        .max_connections(1)
        .register(&mut container)
        .await
        .unwrap();

    let provider = container.resolve::<SqlitePoolProvider>().unwrap();
    sqlx::query("SELECT 1")
        .execute(provider.pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn sqlite_config_from_nidus_config_uses_nested_database_url() {
    let config = nidus_config::Config::from_json_str(
        r#"{"database":{"url":"sqlite::memory:","max_connections":1}}"#,
    )
    .unwrap();

    let settings = SqlitePoolConfig::from_config_path(&config, ["database"]).unwrap();

    assert_eq!(settings.database_url(), "sqlite::memory:");
    assert_eq!(settings.max_connections(), Some(1));
}

#[test]
fn sqlite_module_declares_provider_and_export() {
    let module = ModuleBuilder::new("DatabaseModule")
        .provider_typed::<SqlitePoolProvider>()
        .export_typed::<SqlitePoolProvider>()
        .build();

    assert_eq!(module.providers(), ["SqlitePoolProvider"]);
    assert_eq!(module.exports(), ["SqlitePoolProvider"]);
}

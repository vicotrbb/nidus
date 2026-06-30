#![cfg(feature = "postgres")]

use nidus_core::ModuleBuilder;
use nidus_sqlx::{PostgresPoolConfig, PostgresPoolProvider};

#[test]
fn postgres_provider_preserves_raw_sqlx_options_and_module_metadata() {
    let config = PostgresPoolConfig::new("postgres://localhost/nidus")
        .with_max_connections(5)
        .with_min_connections(1);

    assert_eq!(config.database_url(), "postgres://localhost/nidus");
    assert_eq!(config.max_connections(), Some(5));
    assert_eq!(config.min_connections(), Some(1));

    let module = ModuleBuilder::new("DatabaseModule")
        .provider("PostgresPoolProvider")
        .export_typed::<PostgresPoolProvider>()
        .build();

    assert_eq!(module.providers(), ["PostgresPoolProvider"]);
    assert_eq!(module.exports(), ["PostgresPoolProvider"]);
    assert!(
        module.provider_registrars().is_empty(),
        "SQLx pools require explicit async builder/initializer registration"
    );
}

#[test]
fn postgres_config_from_nidus_config_uses_nested_pool_settings() {
    let config = nidus_config::Config::from_json_str(
        r#"{"database":{"url":"postgres://localhost/nidus","max_connections":5,"min_connections":1}}"#,
    )
    .unwrap();

    let settings = PostgresPoolConfig::from_config_path(&config, ["database"]).unwrap();

    assert_eq!(settings.database_url(), "postgres://localhost/nidus");
    assert_eq!(settings.max_connections(), Some(5));
    assert_eq!(settings.min_connections(), Some(1));
}

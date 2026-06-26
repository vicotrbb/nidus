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
        .provider_typed::<PostgresPoolProvider>()
        .export_typed::<PostgresPoolProvider>()
        .build();

    assert_eq!(module.providers(), ["PostgresPoolProvider"]);
    assert_eq!(module.exports(), ["PostgresPoolProvider"]);
}

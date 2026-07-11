#![cfg(feature = "mysql")]

use nidus_sqlx::{MySqlPoolConfig, MySqlPoolProvider};

#[test]
fn mysql_config_is_typed_redacted_and_exposes_native_builder() {
    fn assert_lifecycle<T: nidus_core::LifecycleHook>() {}
    assert_lifecycle::<MySqlPoolProvider>();
    let config = MySqlPoolConfig::new("mysql://user:secret@localhost/app")
        .allow_insecure_for_local_development()
        .with_min_connections(1)
        .with_max_connections(8);
    assert_eq!(config.min_connections(), Some(1));
    assert_eq!(config.max_connections(), Some(8));
    assert!(!format!("{config:?}").contains("secret"));
    let _builder = MySqlPoolProvider::builder(config.database_url()).config(config);

    assert!(
        MySqlPoolConfig::new("mysql://localhost/app")
            .allow_insecure_for_local_development()
            .with_max_connections(0)
            .validate()
            .is_err()
    );
}

#[cfg(feature = "nidus-config")]
#[test]
fn mysql_config_loads_from_nidus_config() {
    let config = nidus_config::Config::from_json_str(
        r#"{"database":{"url":"mysql://localhost/app","max_connections":12,"allow_insecure_local":true}}"#,
    )
    .unwrap();
    let pool = MySqlPoolConfig::from_config_path(&config, ["database"]).unwrap();
    assert_eq!(pool.max_connections(), Some(12));
}

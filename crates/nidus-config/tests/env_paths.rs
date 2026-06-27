use serde::Deserialize;

use nidus_config::Config;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct EnvConfig {
    name: String,
    port: u16,
    debug: bool,
    database: DatabaseConfig,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct DatabaseConfig {
    url: String,
    #[serde(default)]
    pool_size: u16,
}

#[test]
fn config_loads_prefixed_environment_style_variables() {
    let config = Config::from_prefixed_vars(
        "APP",
        [
            ("APP_NAME", "nidus"),
            ("APP_PORT", "3000"),
            ("APP_DEBUG", "true"),
            ("APP_DATABASE__URL", "postgres://localhost/nidus"),
            ("OTHER_NAME", "ignored"),
        ],
    );

    let settings = config.deserialize::<EnvConfig>().unwrap();

    assert_eq!(
        settings,
        EnvConfig {
            name: "nidus".to_owned(),
            port: 3000,
            debug: true,
            database: DatabaseConfig {
                url: "postgres://localhost/nidus".to_owned(),
                pool_size: 0,
            },
        }
    );
}

#[test]
fn config_exposes_nested_raw_values_by_path() {
    let config = Config::from_prefixed_vars(
        "APP",
        [
            ("APP_DATABASE__URL", "postgres://localhost/nidus"),
            ("APP_DATABASE__POOL_SIZE", "5"),
        ],
    );

    assert_eq!(
        config
            .get_path(["database", "url"])
            .and_then(serde_json::Value::as_str),
        Some("postgres://localhost/nidus")
    );
    assert_eq!(
        config
            .get_path(["database", "pool_size"])
            .and_then(serde_json::Value::as_i64),
        Some(5)
    );
    assert!(config.get_path(["database", "missing"]).is_none());
    assert!(config.get_path(["database", "url", "host"]).is_none());
}

#[test]
fn config_exposes_array_values_by_path_index() {
    let config = Config::from_json_str(
        r#"{
            "servers": [
                { "name": "primary", "port": 3000 },
                { "name": "replica", "port": 3001 }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(
        config
            .get_path(["servers", "0", "name"])
            .and_then(serde_json::Value::as_str),
        Some("primary")
    );
    assert_eq!(
        config
            .get_path(["servers", "1", "port"])
            .and_then(serde_json::Value::as_i64),
        Some(3001)
    );
    assert!(config.get_path(["servers", "2", "name"]).is_none());
    assert!(config.get_path(["servers", "primary", "name"]).is_none());
}

#[test]
fn config_deserializes_nested_values_by_path() {
    let config = Config::from_prefixed_vars(
        "APP",
        [
            ("APP_DATABASE__URL", "postgres://localhost/nidus"),
            ("APP_DATABASE__POOL_SIZE", "5"),
        ],
    );

    assert_eq!(
        config
            .get_path_typed::<_, _, String>(["database", "url"])
            .unwrap(),
        Some("postgres://localhost/nidus".to_owned())
    );
    assert_eq!(
        config
            .get_path_typed::<_, _, u16>(["database", "pool_size"])
            .unwrap(),
        Some(5)
    );
    assert_eq!(
        config
            .get_path_typed::<_, _, String>(["database", "missing"])
            .unwrap(),
        None
    );
}

#[test]
fn config_deserializes_array_values_by_path_index() {
    let config = Config::from_json_str(
        r#"{
            "servers": [
                { "name": "primary", "port": 3000 },
                { "name": "replica", "port": 3001 }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(
        config
            .get_path_typed::<_, _, String>(["servers", "1", "name"])
            .unwrap(),
        Some("replica".to_owned())
    );
    assert_eq!(
        config
            .get_required_path_typed::<_, _, u16>(["servers", "0", "port"])
            .unwrap(),
        3000
    );
}

#[test]
fn config_deserializes_required_nested_values_by_path() {
    let config =
        Config::from_prefixed_vars("APP", [("APP_DATABASE__URL", "postgres://localhost/nidus")]);

    assert_eq!(
        config
            .get_required_path_typed::<_, _, String>(["database", "url"])
            .unwrap(),
        "postgres://localhost/nidus"
    );
}

#[test]
fn config_reports_missing_required_nested_values() {
    let config = Config::from_prefixed_vars("APP", [("APP_DATABASE__URL", "postgres")]);

    let error = config
        .get_required_path_typed::<_, _, u16>(["database", "pool_size"])
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "missing required configuration value `database.pool_size`"
    );
}

#[test]
fn config_reports_typed_path_deserialization_path() {
    let config = Config::from_prefixed_vars("APP", [("APP_DATABASE__POOL_SIZE", "70000")]);

    let error = config
        .get_path_typed::<_, _, u16>(["database", "pool_size"])
        .unwrap_err();

    assert!(error.to_string().contains("database.pool_size"));
}

#[test]
fn config_merges_sources_with_later_values_taking_precedence() {
    let mut defaults =
        Config::from_pairs([("name", "nidus"), ("port", "3000"), ("debug", "false")]);
    defaults.insert_value(
        "database",
        serde_json::json!({
            "url": "postgres://localhost/default",
            "pool_size": 5,
        }),
    );
    let overrides = Config::from_prefixed_vars(
        "APP",
        [
            ("APP_PORT", "4000"),
            ("APP_DATABASE__URL", "postgres://localhost/override"),
        ],
    );

    let settings = defaults
        .merge(overrides)
        .deserialize::<EnvConfig>()
        .unwrap();

    assert_eq!(
        settings,
        EnvConfig {
            name: "nidus".to_owned(),
            port: 4000,
            debug: false,
            database: DatabaseConfig {
                url: "postgres://localhost/override".to_owned(),
                pool_size: 5,
            },
        }
    );
}

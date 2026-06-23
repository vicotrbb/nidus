use serde::Deserialize;

use nidus_config::Config;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct AppConfig {
    name: String,
    port: u16,
    debug: bool,
}

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
fn config_deserializes_typed_settings_from_pairs() {
    let config = Config::from_pairs([("name", "nidus"), ("port", "3000"), ("debug", "true")]);

    let settings = config.deserialize::<AppConfig>().unwrap();

    assert_eq!(
        settings,
        AppConfig {
            name: "nidus".to_owned(),
            port: 3000,
            debug: true,
        }
    );
}

#[test]
fn config_deserializes_typed_settings_from_json_object() {
    let config = Config::from_json_str(
        r#"{
            "name": "nidus",
            "port": 3000,
            "debug": true,
            "database": {
                "url": "postgres://localhost/nidus",
                "pool_size": 8
            }
        }"#,
    )
    .unwrap();

    let settings = config.deserialize::<EnvConfig>().unwrap();

    assert_eq!(
        settings,
        EnvConfig {
            name: "nidus".to_owned(),
            port: 3000,
            debug: true,
            database: DatabaseConfig {
                url: "postgres://localhost/nidus".to_owned(),
                pool_size: 8,
            },
        }
    );
}

#[test]
fn config_rejects_invalid_json_sources() {
    let error = Config::from_json_str("{not-json").unwrap_err();

    assert!(error.to_string().contains("configuration JSON parse error"));
}

#[test]
fn config_rejects_non_object_json_roots() {
    let error = Config::from_json_str("[\"nidus\"]").unwrap_err();

    assert_eq!(
        error.to_string(),
        "configuration root must be a JSON object"
    );
}

#[test]
fn config_exposes_top_level_raw_values() {
    let config = Config::from_pairs([("name", "nidus"), ("port", "3000"), ("debug", "true")]);

    assert_eq!(
        config.get("name").and_then(serde_json::Value::as_str),
        Some("nidus")
    );
    assert_eq!(
        config.get("port").and_then(serde_json::Value::as_i64),
        Some(3000)
    );
    assert_eq!(
        config.get("debug").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(config.get("missing").is_none());
}

#[test]
fn config_deserializes_top_level_values_by_key() {
    let config = Config::from_pairs([("port", "3000"), ("debug", "true")]);

    assert_eq!(config.get_typed::<u16>("port").unwrap(), Some(3000));
    assert_eq!(config.get_typed::<bool>("debug").unwrap(), Some(true));
    assert_eq!(config.get_typed::<String>("missing").unwrap(), None);
}

#[test]
fn config_reports_typed_key_deserialization_path() {
    let config = Config::from_pairs([("port", "70000")]);

    let error = config.get_typed::<u16>("port").unwrap_err();

    assert!(error.to_string().contains("port"));
}

#[test]
fn config_reports_missing_typed_fields() {
    let config = Config::from_pairs([("name", "nidus")]);
    let error = config.deserialize::<AppConfig>().unwrap_err();

    assert!(error.to_string().contains("port"));
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

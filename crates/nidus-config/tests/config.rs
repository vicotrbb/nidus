use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde::Deserialize;

use nidus_config::Config;

static NEXT_TEST_FILE_ID: AtomicUsize = AtomicUsize::new(0);

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
fn config_supports_repeated_typed_deserialization_without_consuming_values() {
    let config = Config::from_pairs([("name", "nidus"), ("port", "3000"), ("debug", "true")]);

    let first = config.deserialize::<AppConfig>().unwrap();
    let second = config.deserialize::<AppConfig>().unwrap();

    assert_eq!(first, second);
    assert_eq!(config.get_typed::<u16>("port").unwrap(), Some(3000));
    assert_eq!(
        config.get("name").and_then(serde_json::Value::as_str),
        Some("nidus")
    );
}

#[test]
fn config_deserializes_typed_settings_from_json_file() {
    let path = write_temp_config(
        "valid",
        r#"{
            "name": "nidus",
            "port": 3000,
            "debug": true
        }"#,
    );

    let config = Config::from_json_file(&path).unwrap();
    let settings = config.deserialize::<AppConfig>().unwrap();
    fs::remove_file(&path).unwrap();

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
fn config_reports_missing_json_file_path() {
    let path = temp_config_path("missing");

    let error = Config::from_json_file(&path).unwrap_err();

    assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
    assert!(error.to_string().contains("read error"));
}

#[test]
fn config_reports_invalid_json_file_path() {
    let path = write_temp_config("invalid-json", "{not-json");

    let error = Config::from_json_file(&path).unwrap_err();
    fs::remove_file(&path).unwrap();

    assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
    assert!(error.to_string().contains("JSON parse error"));
}

#[test]
fn config_rejects_non_object_json_file_roots() {
    let path = write_temp_config("array-root", "[\"nidus\"]");

    let error = Config::from_json_file(&path).unwrap_err();
    fs::remove_file(&path).unwrap();

    assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
    assert!(error.to_string().contains("root must be a JSON object"));
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
fn config_deserializes_required_top_level_values_by_key() {
    let config = Config::from_pairs([("port", "3000")]);

    assert_eq!(config.get_required_typed::<u16>("port").unwrap(), 3000);
}

#[test]
fn config_reports_missing_required_top_level_values() {
    let config = Config::new();

    let error = config
        .get_required_typed::<String>("database_url")
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "missing required configuration value `database_url`"
    );
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

fn write_temp_config(label: &str, contents: &str) -> PathBuf {
    let path = temp_config_path(label);
    fs::write(&path, contents).unwrap();
    path
}

fn temp_config_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "nidus-config-{label}-{}-{}.json",
        std::process::id(),
        NEXT_TEST_FILE_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

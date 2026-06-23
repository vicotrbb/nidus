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

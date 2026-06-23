use serde::Deserialize;

use nidus_config::Config;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct AppConfig {
    name: String,
    port: u16,
    debug: bool,
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

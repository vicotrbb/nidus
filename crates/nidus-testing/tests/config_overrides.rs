use axum::Router;
use nidus_config::Config;
use nidus_testing::TestApp;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct AppConfig {
    database_url: String,
    feature_enabled: bool,
}

#[test]
fn test_app_builder_accepts_config_overrides() {
    let app = TestApp::builder(Router::new())
        .config(Config::from_pairs([
            ("database_url", "postgres://localhost/test"),
            ("feature_enabled", "true"),
        ]))
        .build();

    let config = app.config().deserialize::<AppConfig>().unwrap();

    assert_eq!(
        config,
        AppConfig {
            database_url: "postgres://localhost/test".to_owned(),
            feature_enabled: true,
        }
    );
}

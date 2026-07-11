use std::time::Duration;

use nidus_redis::{RedisConfig, RedisError, RedisProvider};

#[test]
fn config_debug_redacts_credentials_and_builds_native_client() {
    let config = RedisConfig::new("redis://user:secret@localhost:6379/0")
        .allow_plaintext_for_local_development()
        .with_concurrency_limit(32)
        .with_pipeline_buffer_size(128);
    let debug = format!("{config:?}");
    assert!(!debug.contains("secret"));
    assert!(debug.contains("<redacted>"));

    let builder = RedisProvider::builder(config.url()).config(config);
    assert!(builder.build_client().is_ok());

    assert!(
        RedisConfig::new("redis://127.0.0.1:16379/0")
            .allow_plaintext_for_local_development()
            .validate()
            .is_ok()
    );
}

#[test]
fn zero_bounds_are_rejected_before_network_io() {
    let error = RedisConfig::new("redis://127.0.0.1/")
        .allow_plaintext_for_local_development()
        .with_response_timeout(Duration::ZERO)
        .validate()
        .unwrap_err();
    assert!(matches!(
        error,
        RedisError::InvalidBound {
            field: "response_timeout"
        }
    ));

    assert!(
        RedisConfig::new("redis://127.0.0.1/")
            .allow_plaintext_for_local_development()
            .with_reconnect_backoff(Duration::from_secs(2), Duration::from_secs(1))
            .validate()
            .is_err()
    );
    assert!(
        RedisConfig::new("redis://localhost:6379@evil.example:6379/0")
            .allow_plaintext_for_local_development()
            .validate()
            .is_err()
    );
}

#[cfg(feature = "nidus-config")]
#[test]
fn config_loads_from_typed_nidus_path() {
    let config = nidus_config::Config::from_json_str(
        r#"{"redis":{"url":"redis://localhost:6379/1","concurrency_limit":64,"response_timeout_ms":1500,"allow_plaintext_local":true}}"#,
    )
    .unwrap();
    let redis = RedisConfig::from_config_path(&config, ["redis"]).unwrap();
    assert_eq!(redis.url(), "redis://localhost:6379/1");
}

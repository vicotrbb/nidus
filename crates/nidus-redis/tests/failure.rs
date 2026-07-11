use std::time::Duration;

use nidus_redis::{RedisConfig, RedisProvider};

#[tokio::test]
async fn connection_failure_is_bounded_and_preserves_redis_error() {
    let config = RedisConfig::new("redis://127.0.0.1:1/")
        .allow_plaintext_for_local_development()
        .with_connection_timeout(Duration::from_millis(50))
        .with_response_timeout(Duration::from_millis(50))
        .with_reconnect_attempts(0);
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        RedisProvider::builder(config.url())
            .config(config)
            .connect(),
    )
    .await;
    assert!(
        result.is_ok(),
        "connect attempt exceeded its configured bound"
    );
    assert!(result.unwrap().is_err());
}

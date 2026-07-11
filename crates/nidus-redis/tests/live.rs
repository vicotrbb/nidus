use std::time::Duration;

use nidus_redis::{RedisConfig, RedisProvider};

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_redis_round_trip_ttl_health_and_cleanup() {
    let url = std::env::var("NIDUS_TEST_REDIS_URL").expect("integration URL is required");
    let config = RedisConfig::new(&url)
        .allow_plaintext_for_local_development()
        .with_connection_timeout(Duration::from_secs(2))
        .with_response_timeout(Duration::from_secs(2));
    let provider = RedisProvider::builder(&url)
        .config(config)
        .connect()
        .await
        .unwrap();
    let debug = format!("{provider:?}");
    assert!(!debug.contains(&url));
    assert!(debug.contains("<redacted>"));
    let key = format!("nidus:test:{}", uuid::Uuid::new_v4());

    provider
        .set(&key, b"value", Some(Duration::from_secs(30)))
        .await
        .unwrap();
    if std::env::var("NIDUS_TEST_INJECT_PANIC").as_deref() == Ok("1") {
        panic!("injected integration-test panic after creating a Redis key");
    }
    assert_eq!(provider.get(&key).await.unwrap(), Some(b"value".to_vec()));
    assert!(provider.delete(&key).await.unwrap());
    assert_eq!(provider.get(&key).await.unwrap(), None);
    let _native_client: &redis::Client = provider.client();
    provider.shutdown().await.unwrap();
    assert!(matches!(
        provider.get(&key).await,
        Err(nidus_redis::RedisError::ShuttingDown)
    ));
    provider.shutdown().await.unwrap();
}

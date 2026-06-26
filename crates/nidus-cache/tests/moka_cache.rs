use std::time::Duration;

use nidus_cache::{CacheConfig, MokaCacheProvider};
use nidus_core::Container;

#[tokio::test]
async fn moka_cache_namespaces_keys_and_expires_values() {
    let cache = MokaCacheProvider::builder()
        .namespace("users")
        .time_to_live(Duration::from_millis(50))
        .max_capacity(100)
        .build();

    cache.insert("42", b"Ada".to_vec()).await;

    assert_eq!(cache.get("42").await.unwrap(), b"Ada".to_vec());
    assert!(cache.get("users:42").await.is_none());

    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(cache.get("42").await.is_none());
}

#[tokio::test]
async fn moka_cache_registers_in_container() {
    let mut container = Container::new();
    let config = CacheConfig::new().namespace("sessions");

    MokaCacheProvider::builder()
        .config(config)
        .register(&mut container)
        .unwrap();

    let cache = container.resolve::<MokaCacheProvider>().unwrap();
    cache.insert("abc", b"token".to_vec()).await;
    assert_eq!(cache.get("abc").await.unwrap(), b"token".to_vec());
}

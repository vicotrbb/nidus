use std::time::Duration;

use nidus_cache::{CacheConfig, MokaCacheProvider};
use nidus_core::{Container, ProviderRegistrant};

#[cfg(feature = "health")]
use {nidus_http::health::HealthRegistry, std::sync::Arc};

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

#[tokio::test]
async fn moka_cache_provider_registrant_installs_default_cache() {
    let mut container = Container::new();

    MokaCacheProvider::register_provider(&mut container).unwrap();

    let cache = container.resolve::<MokaCacheProvider>().unwrap();
    cache.insert("abc", b"token".to_vec()).await;
    assert_eq!(cache.get("abc").await.unwrap(), b"token".to_vec());
}

#[tokio::test]
async fn moka_cache_invalidate_removes_only_the_targeted_key() {
    let cache = MokaCacheProvider::builder().namespace("users").build();
    cache.insert("42", b"Ada".to_vec()).await;
    cache.insert("43", b"Bob".to_vec()).await;

    cache.invalidate("42").await;

    assert!(
        cache.get("42").await.is_none(),
        "target key must be removed"
    );
    assert_eq!(
        cache.get("43").await.unwrap(),
        b"Bob".to_vec(),
        "other keys must be untouched"
    );
}

#[tokio::test]
async fn moka_cache_from_cache_wraps_an_existing_moka_instance() {
    // AD-3: from_cache wraps a caller-owned Moka cache and applies the namespace
    // to logical keys, so pre-populated values are reachable through the provider.
    let raw = moka::future::Cache::<String, Vec<u8>>::builder().build();
    raw.insert("users:42".to_owned(), b"Ada".to_vec()).await;

    let cache = MokaCacheProvider::from_cache(raw, Some("users".to_owned()));
    assert_eq!(cache.namespace(), Some("users"));
    assert_eq!(cache.get("42").await.unwrap(), b"Ada".to_vec());
}

#[cfg(feature = "health")]
#[test]
fn moka_cache_registers_ready_check_with_health_registry() {
    let cache = Arc::new(MokaCacheProvider::builder().build());

    let registry = cache.register_ready_check(HealthRegistry::new(), "cache");

    let _routes = registry.routes();
}

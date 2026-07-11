use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nidus_jobs::{DurableJobStore, LeaseRequest, NewJob};
use nidus_jobs_sqlx::{SqlxJobStore, SqlxJobStoreConfig};
use serde_json::json;

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_mysql_store_is_multi_worker_safe() {
    let url = std::env::var("NIDUS_TEST_JOBS_MYSQL_URL").expect("integration URL is required");
    let config = SqlxJobStoreConfig::mysql(url)
        .allow_insecure_local_mysql()
        .unwrap()
        .with_max_connections(4)
        .unwrap();
    exercise_store(config).await;
}

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_cockroach_tls_store_is_multi_worker_safe() {
    let url = std::env::var("NIDUS_TEST_JOBS_COCKROACH_URL").expect("integration URL is required");
    let config = SqlxJobStoreConfig::cockroach(url)
        .with_max_connections(4)
        .unwrap();
    exercise_store(config).await;
}

async fn exercise_store(config: SqlxJobStoreConfig) {
    let store = SqlxJobStore::connect(config).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS nidus_jobs")
        .execute(store.pool())
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let job = NewJob::new("live.execute", json!({"value": 1}))
        .unwrap()
        .with_idempotency_key("live-once")
        .unwrap();
    assert!(store.enqueue(job.clone()).await.unwrap().was_enqueued());
    assert!(!store.enqueue(job).await.unwrap().was_enqueued());

    let now = now_ms();
    let first_store = store.clone();
    let second_store = store.clone();
    let (first, second) = tokio::join!(
        first_store
            .lease(LeaseRequest::new("live-worker-a", now, Duration::from_secs(5), 1).unwrap()),
        second_store
            .lease(LeaseRequest::new("live-worker-b", now, Duration::from_secs(5), 1).unwrap())
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.len() + second.len(), 1);
    let leased = first.into_iter().chain(second).next().unwrap();
    assert!(
        store
            .acknowledge(
                &leased.id,
                leased.lease_owner.as_deref().unwrap(),
                leased.attempts,
            )
            .await
            .unwrap()
    );
    assert_eq!(store.stats().await.unwrap().succeeded, 1);

    sqlx::query("DROP TABLE nidus_jobs")
        .execute(store.pool())
        .await
        .unwrap();
    store.close().await;
}

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

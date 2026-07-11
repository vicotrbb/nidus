#[cfg(feature = "cockroach")]
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[cfg(feature = "cockroach")]
use nidus_sqlx::{CockroachPoolConfig, CockroachPoolProvider, CockroachRetryPolicy};
#[cfg(feature = "mysql")]
use nidus_sqlx::{MySqlPoolConfig, MySqlPoolProvider};

#[cfg(feature = "mysql")]
#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_mysql_pool_round_trip_and_cleanup() {
    let url = std::env::var("NIDUS_TEST_MYSQL_URL").expect("integration URL is required");
    let config = MySqlPoolConfig::new(url)
        .with_max_connections(2)
        .allow_insecure_for_local_development();
    let provider = MySqlPoolProvider::builder(config.database_url())
        .config(config)
        .connect()
        .await
        .unwrap();

    sqlx::query("DROP TABLE IF EXISTS nidus_live_mysql")
        .execute(provider.pool())
        .await
        .unwrap();
    sqlx::query("CREATE TABLE nidus_live_mysql (id BIGINT PRIMARY KEY, value_text VARCHAR(64))")
        .execute(provider.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO nidus_live_mysql (id, value_text) VALUES (?, ?)")
        .bind(1_i64)
        .bind("round-trip")
        .execute(provider.pool())
        .await
        .unwrap();
    let value: String = sqlx::query_scalar("SELECT value_text FROM nidus_live_mysql WHERE id = ?")
        .bind(1_i64)
        .fetch_one(provider.pool())
        .await
        .unwrap();
    assert_eq!(value, "round-trip");
    sqlx::query("DROP TABLE nidus_live_mysql")
        .execute(provider.pool())
        .await
        .unwrap();
    provider.pool().close().await;
}

#[cfg(feature = "cockroach")]
#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_cockroach_verify_full_tls_and_injected_serialization_retries() {
    let url = std::env::var("NIDUS_TEST_COCKROACH_URL").expect("integration URL is required");
    let retry_policy = CockroachRetryPolicy::new()
        .with_max_attempts(4)
        .without_jitter();
    let config = CockroachPoolConfig::new(url)
        .with_max_connections(1)
        .with_retry_policy(retry_policy);
    let provider = CockroachPoolProvider::builder(config.database_url())
        .config(config)
        .connect()
        .await
        .unwrap();

    sqlx::query("DROP TABLE IF EXISTS nidus_live_cockroach")
        .execute(provider.pool())
        .await
        .unwrap();
    sqlx::query("CREATE TABLE nidus_live_cockroach (id INT8 PRIMARY KEY, value_text STRING)")
        .execute(provider.pool())
        .await
        .unwrap();

    sqlx::query("SET inject_retry_errors_enabled = true")
        .execute(provider.pool())
        .await
        .unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let transaction_attempts = Arc::clone(&attempts);
    provider
        .transaction_with_retry(move |connection| {
            let attempt = transaction_attempts.fetch_add(1, Ordering::SeqCst) + 1;
            Box::pin(async move {
                if attempt == 3 {
                    sqlx::query("SET inject_retry_errors_enabled = false")
                        .execute(&mut *connection)
                        .await?;
                }
                sqlx::query("UPSERT INTO nidus_live_cockroach (id, value_text) VALUES ($1, $2)")
                    .bind(1_i64)
                    .bind("retried")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .await
        .unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 3);

    let value: String =
        sqlx::query_scalar("SELECT value_text FROM nidus_live_cockroach WHERE id = $1")
            .bind(1_i64)
            .fetch_one(provider.pool())
            .await
            .unwrap();
    assert_eq!(value, "retried");
    sqlx::query("DROP TABLE nidus_live_cockroach")
        .execute(provider.pool())
        .await
        .unwrap();
    provider.pool().close().await;
}

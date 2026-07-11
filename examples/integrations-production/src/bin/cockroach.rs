use nidus_sqlx::{CockroachPoolConfig, CockroachPoolProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("COCKROACH_DATABASE_URL")?;
    let mut config = CockroachPoolConfig::new(&url).with_max_connections(10);
    if local_plaintext_enabled() {
        config = config.allow_insecure_for_local_development();
    }
    let provider = CockroachPoolProvider::builder(url)
        .config(config)
        .connect()
        .await?;
    let value = provider
        .transaction_with_retry(|connection| {
            Box::pin(async move {
                sqlx::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(connection)
                    .await
            })
        })
        .await?;
    println!("CockroachDB retry-safe transaction returned {value}");
    provider.pool().close().await;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

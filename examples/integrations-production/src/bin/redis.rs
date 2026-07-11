use nidus_redis::{RedisConfig, RedisProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("REDIS_URL")?;
    let mut config = RedisConfig::new(url);
    if local_plaintext_enabled() {
        config = config.allow_plaintext_for_local_development();
    }
    let provider = RedisProvider::builder(config.url().to_owned())
        .config(config)
        .connect()
        .await?;
    println!("Redis readiness: {:?}", provider.health_status().await);
    let _native_client = provider.client();
    let _reconnecting_connection = provider.connection();
    provider.shutdown().await?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

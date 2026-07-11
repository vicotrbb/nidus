use nidus_sqlx::{MySqlPoolConfig, MySqlPoolProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("MYSQL_DATABASE_URL")?;
    let mut config = MySqlPoolConfig::new(&url).with_max_connections(10);
    if local_plaintext_enabled() {
        config = config.allow_insecure_for_local_development();
    }
    let provider = MySqlPoolProvider::builder(url)
        .config(config)
        .connect()
        .await?;
    let value: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(provider.pool())
        .await?;
    println!("MySQL native SQLx query returned {value}");
    provider.pool().close().await;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

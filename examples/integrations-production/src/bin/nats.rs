use nidus_nats::{NatsConfig, NatsProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("NATS_URL")?;
    let mut config = NatsConfig::new(url, "nidus-example");
    if local_plaintext_enabled() {
        config = config.allow_plaintext_for_local_development();
    }
    let provider = NatsProvider::builder(config).connect().await?;
    println!("NATS readiness: {:?}", provider.health_status().await);
    let _native_core_client = provider.client();
    let _native_jetstream = provider.jetstream();
    provider.shutdown().await?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

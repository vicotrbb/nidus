use nidus_rabbitmq::{RabbitMqConfig, RabbitMqProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("RABBITMQ_URL")?;
    let mut config = RabbitMqConfig::new(url);
    if local_plaintext_enabled() {
        config = config.allow_plaintext_for_local_development();
    }
    let provider = RabbitMqProvider::builder(config).connect().await?;
    println!("RabbitMQ readiness: {:?}", provider.health_status().await);
    let _native_connection = provider.connection();
    let _native_confirm_channel = provider.channel();
    provider.shutdown().await?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

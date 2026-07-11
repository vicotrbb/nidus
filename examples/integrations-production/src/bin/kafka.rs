use nidus_kafka::{KafkaConfig, KafkaProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let brokers = std::env::var("KAFKA_BOOTSTRAP_SERVERS")?;
    let mut config = KafkaConfig::new(brokers, "nidus-example");
    if local_plaintext_enabled() {
        config = config.allow_plaintext_for_local_development();
    }
    let provider = KafkaProvider::builder(config).build()?;
    println!("Kafka readiness: {:?}", provider.health_status().await);
    let _native_admin = provider.admin();
    let _native_producer = provider.producer();
    provider.shutdown().await?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

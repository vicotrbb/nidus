use nidus_sqs::{SqsConfig, SqsProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let queue_url = std::env::var("SQS_QUEUE_URL")?;
    let mut config = SqsConfig::new(queue_url)
        .with_wait_time_seconds(20)
        .with_visibility_timeout_seconds(60)
        .with_max_messages(10);
    if local_plaintext_enabled() {
        config = config.allow_http_for_local_development();
    }
    let provider = SqsProvider::builder(config).connect().await?;
    println!("SQS readiness: {:?}", provider.health_status().await);
    let _native_aws_client = provider.client();
    provider.shutdown().await?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

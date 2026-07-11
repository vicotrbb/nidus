use nidus_opentelemetry::{OpenTelemetryConfig, OpenTelemetryPipeline};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = OpenTelemetryConfig::from_env("nidus-example")?
        .with_service_version(env!("CARGO_PKG_VERSION"))
        .with_environment("example");
    if local_plaintext_enabled() {
        config = config.allow_insecure_local_endpoint()?;
    }
    let pipeline = OpenTelemetryPipeline::init(config)?;
    let subscriber = tracing_subscriber::registry().with(pipeline.tracing_layer());
    {
        let _default = tracing::subscriber::set_default(subscriber);
        let span = tracing::info_span!("example.operation");
        let _entered = span.enter();
        tracing::info!("exported through the bounded OTLP batch processor");
    }
    let flush = pipeline.force_flush().await;
    let shutdown = pipeline.shutdown().await;
    flush?;
    shutdown?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

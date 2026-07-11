use axum::{Router, body::Body, routing::get};
use nidus_sentry::{SentryConfig, SentryIntegration};
use sentry::Level;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = SentryConfig::from_env()?
        .with_release(env!("CARGO_PKG_VERSION"))
        .with_environment("example");
    if local_plaintext_enabled() {
        config = config.allow_insecure_local_dsn()?;
    }
    let integration = SentryIntegration::init(config)?;
    let _router: Router<()> = Router::new()
        .route("/health", get(|| async { "ok" }))
        .layer(integration.tower_layer::<Body>());
    let subscriber = tracing_subscriber::registry().with(integration.tracing_layer());
    {
        let _default = tracing::subscriber::set_default(subscriber);
        tracing::warn!("warning breadcrumb from tracing");
        integration.capture_message("Sentry example event", Level::Error);
    }
    let flush = integration.flush().await;
    let shutdown = integration.shutdown().await;
    flush?;
    shutdown?;
    Ok(())
}

fn local_plaintext_enabled() -> bool {
    std::env::var_os("NIDUS_ALLOW_LOCAL_PLAINTEXT").is_some()
}

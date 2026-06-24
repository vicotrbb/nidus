use std::sync::Once;

use nidus::prelude::{LoggingConfig, LoggingFormat};

use crate::config::AppConfig;

static INIT: Once = Once::new();

pub fn logging_config(config: &AppConfig) -> LoggingConfig {
    let format = match config.log_format.as_str() {
        "json" | "production" => LoggingFormat::Json,
        _ => LoggingFormat::Pretty,
    };

    LoggingConfig::production("nidus-realworld-api")
        .version(env!("CARGO_PKG_VERSION"))
        .environment(config.environment.clone())
        .with_format(format)
        .redact_header("authorization")
        .redact_header("x-api-key")
}

pub fn init(config: &AppConfig) {
    let logging = logging_config(config);
    INIT.call_once(|| {
        let _ = logging
            .level_filter(
                std::env::var("RUST_LOG")
                    .unwrap_or_else(|_| "nidus_example_realworld_api=info,tower_http=info".into()),
            )
            .init();
    });
}

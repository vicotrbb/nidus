use nidus_nats::{NatsConfig, NatsError, NatsProvider};

#[test]
fn tls_config_redacts_server_and_rejects_implicit_plaintext() {
    let config = NatsConfig::new("tls://nats.example:4222", "orders-api");
    assert!(config.validate().is_ok());
    assert!(!format!("{config:?}").contains("nats.example"));

    let plaintext = NatsConfig::new("nats://localhost:4222", "test");
    assert!(matches!(
        plaintext.validate(),
        Err(NatsError::Configuration { .. })
    ));
    assert!(
        plaintext
            .allow_plaintext_for_local_development()
            .validate()
            .is_ok()
    );
    assert!(
        NatsConfig::new("nats://127.0.0.1:14222", "test")
            .allow_plaintext_for_local_development()
            .validate()
            .is_ok()
    );
    assert!(
        NatsConfig::new("tls://nats.example:4222", "client")
            .with_shutdown_timeout(std::time::Duration::ZERO)
            .validate()
            .is_err()
    );
    assert!(
        NatsConfig::new("nats://localhost:4222@evil.example:4222", "client")
            .allow_plaintext_for_local_development()
            .validate()
            .is_err()
    );
}

#[test]
fn provider_builder_accepts_native_connect_options() {
    let config = NatsConfig::new("tls://nats.example:4222", "orders-api");
    let builder = NatsProvider::builder(config)
        .connect_options(async_nats::ConnectOptions::new().token("secret".to_owned()));
    let debug = format!("{builder:?}");
    assert!(!debug.contains("secret"));
    assert!(debug.contains("<redacted>"));
}

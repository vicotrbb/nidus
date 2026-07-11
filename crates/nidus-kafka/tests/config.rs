use nidus_kafka::{KafkaConfig, KafkaError, KafkaProvider};

#[tokio::test]
async fn secure_config_redacts_endpoints_and_builds_native_clients() {
    let config = KafkaConfig::new("broker.example:9093", "orders-api")
        .property("ssl.ca.location", "/run/secrets/kafka-ca.pem")
        .property("sasl.password", "super-secret");
    let debug = format!("{config:?}");
    assert!(!debug.contains("broker.example"));
    assert!(!debug.contains("super-secret"));
    assert!(debug.contains("sasl.password"));

    let provider = KafkaProvider::builder(
        KafkaConfig::new("localhost:9092", "orders-api").allow_plaintext_for_local_development(),
    )
    .build()
    .unwrap();
    let _producer = provider.producer();
    let _admin = provider.admin();
    let _consumer = provider.consumer("orders-workers").unwrap();
}

#[tokio::test]
async fn plaintext_and_empty_consumer_groups_require_explicit_handling() {
    let config =
        KafkaConfig::new("localhost:9092", "test").property("security.protocol", "PLAINTEXT");
    assert!(matches!(
        config.validate(),
        Err(KafkaError::Configuration { .. })
    ));

    let provider = KafkaProvider::builder(
        KafkaConfig::new("localhost:9092", "test").allow_plaintext_for_local_development(),
    )
    .build()
    .unwrap();
    assert!(matches!(
        provider.consumer(""),
        Err(KafkaError::Configuration { .. })
    ));
    assert!(
        KafkaConfig::new("localhost:9092@evil.example:9092", "test")
            .allow_plaintext_for_local_development()
            .validate()
            .is_err()
    );
}

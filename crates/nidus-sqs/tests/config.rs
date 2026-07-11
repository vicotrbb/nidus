use nidus_sqs::{SqsConfig, SqsError};

#[test]
fn queue_config_redacts_urls_and_enforces_service_bounds() {
    let config = SqsConfig::new("https://sqs.us-east-1.amazonaws.com/123456789/orders");
    assert!(config.validate().is_ok());
    assert!(!format!("{config:?}").contains("123456789"));

    assert!(matches!(
        config.clone().with_max_messages(11).validate(),
        Err(SqsError::Configuration { .. })
    ));
    assert!(matches!(
        SqsConfig::new("http://localhost:4566/queue/orders").validate(),
        Err(SqsError::Configuration { .. })
    ));
    assert!(
        SqsConfig::new("http://localhost:4566/queue/orders")
            .allow_http_for_local_development()
            .validate()
            .is_ok()
    );
    assert!(
        config
            .with_shutdown_timeout(std::time::Duration::ZERO)
            .validate()
            .is_err()
    );
    assert!(
        SqsConfig::new("http://localhost:4566@evil.example/queue/orders")
            .allow_http_for_local_development()
            .validate()
            .is_err()
    );
}

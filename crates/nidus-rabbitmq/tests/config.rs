use nidus_rabbitmq::{RabbitMqConfig, RabbitMqError, RabbitMqProvider};

#[test]
fn secure_config_redacts_credentials_and_requires_explicit_plaintext() {
    let secure = RabbitMqConfig::new("amqps://user:secret@rabbit.example/%2f");
    assert!(secure.validate().is_ok());
    let debug = format!("{secure:?}");
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("rabbit.example"));

    let plaintext = RabbitMqConfig::new("amqp://guest:guest@localhost/%2f");
    assert!(matches!(
        plaintext.validate(),
        Err(RabbitMqError::Configuration { .. })
    ));
    let _builder = RabbitMqProvider::builder(plaintext.allow_plaintext_for_local_development());
    assert!(
        RabbitMqConfig::new("amqp://guest:guest@127.0.0.1:15672/%2f")
            .allow_plaintext_for_local_development()
            .validate()
            .is_ok()
    );
    assert!(
        RabbitMqConfig::new("amqps://rabbit.example/%2f")
            .with_shutdown_timeout(std::time::Duration::ZERO)
            .validate()
            .is_err()
    );
    assert!(
        RabbitMqConfig::new("amqp://localhost:5672@evil.example:5672/%2f")
            .allow_plaintext_for_local_development()
            .validate()
            .is_err()
    );
}

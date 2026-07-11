use std::time::Duration;

use futures_util::StreamExt;
use nidus_integrations::MessageEnvelope;
use nidus_kafka::{KafkaConfig, KafkaProvider};
use rdkafka::{
    Message,
    admin::{AdminOptions, NewTopic, TopicReplication},
    consumer::{CommitMode, Consumer},
};
use serde_json::json;

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_kafka_admin_delivery_consume_commit_and_cleanup() {
    let brokers = std::env::var("NIDUS_TEST_KAFKA_BROKERS").expect("broker URL is required");
    let config = KafkaConfig::new(&brokers, "nidus-live-test")
        .allow_plaintext_for_local_development()
        .property("auto.offset.reset", "earliest");
    let provider = KafkaProvider::builder(config).build().unwrap();
    let topic = format!("nidus-live-{}", uuid::Uuid::new_v4());

    let created = provider
        .admin()
        .create_topics(
            &[NewTopic::new(&topic, 1, TopicReplication::Fixed(1))],
            &AdminOptions::new().operation_timeout(Some(Duration::from_secs(10))),
        )
        .await
        .unwrap();
    assert!(created[0].is_ok(), "topic creation failed: {created:?}");

    let consumer = provider.consumer("nidus-live-workers").unwrap();
    consumer.subscribe(&[&topic]).unwrap();
    let envelope = MessageEnvelope::new("orders.created", json!({"order_id": 7})).unwrap();
    let delivery = provider
        .publish_envelope(&topic, Some(b"order-7"), &envelope)
        .await
        .unwrap();
    assert!(delivery.offset() >= 0);

    let mut messages = consumer.stream();
    let message = tokio::time::timeout(Duration::from_secs(15), messages.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(message.topic(), topic);
    let decoded: MessageEnvelope<serde_json::Value> =
        MessageEnvelope::from_json(message.payload().unwrap()).unwrap();
    assert_eq!(decoded.id(), envelope.id());
    consumer.commit_message(&message, CommitMode::Sync).unwrap();

    let deleted = provider
        .admin()
        .delete_topics(
            &[&topic],
            &AdminOptions::new().operation_timeout(Some(Duration::from_secs(10))),
        )
        .await
        .unwrap();
    assert!(deleted[0].is_ok(), "topic deletion failed: {deleted:?}");
    provider.shutdown().await.unwrap();
    assert!(matches!(
        provider.publish(&topic, None, b"after-shutdown").await,
        Err(nidus_kafka::KafkaError::ShuttingDown)
    ));
    provider.shutdown().await.unwrap();
}

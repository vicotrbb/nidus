use std::time::Duration;

use futures_util::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, QueueDeclareOptions, QueueDeleteOptions},
    types::FieldTable,
};
use nidus_integrations::MessageEnvelope;
use nidus_rabbitmq::{RabbitMqConfig, RabbitMqProvider};
use serde_json::json;

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_rabbitmq_confirm_consume_ack_and_cleanup() {
    let uri = std::env::var("NIDUS_TEST_RABBITMQ_URL").expect("broker URL is required");
    let provider =
        RabbitMqProvider::builder(RabbitMqConfig::new(uri).allow_plaintext_for_local_development())
            .connect()
            .await
            .unwrap();
    let queue = format!("nidus-live-{}", uuid::Uuid::new_v4());
    provider
        .channel()
        .queue_declare(
            queue.as_str().into(),
            QueueDeclareOptions {
                auto_delete: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await
        .unwrap();
    let mut consumer = provider
        .channel()
        .basic_consume(
            queue.as_str().into(),
            "nidus-live-consumer".into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .unwrap();

    let envelope = MessageEnvelope::new("orders.created", json!({"order_id": 11})).unwrap();
    provider
        .publish_envelope("", &queue, &envelope)
        .await
        .unwrap();
    let delivery = tokio::time::timeout(Duration::from_secs(10), consumer.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let decoded: MessageEnvelope<serde_json::Value> =
        MessageEnvelope::from_json(&delivery.data).unwrap();
    assert_eq!(decoded.id(), envelope.id());
    delivery.ack(BasicAckOptions::default()).await.unwrap();

    drop(consumer);
    provider
        .channel()
        .queue_delete(queue.as_str().into(), QueueDeleteOptions::default())
        .await
        .unwrap();
    provider.shutdown().await.unwrap();
    assert!(matches!(
        provider.publish_envelope("", &queue, &envelope).await,
        Err(nidus_rabbitmq::RabbitMqError::ShuttingDown)
    ));
    provider.shutdown().await.unwrap();
}

use std::time::Duration;

use async_nats::jetstream::{self, consumer::PullConsumer};
use futures_util::StreamExt;
use nidus_integrations::MessageEnvelope;
use nidus_nats::{NatsConfig, NatsProvider};
use serde_json::json;

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_jetstream_persistence_durable_consumer_ack_and_cleanup() {
    let url = std::env::var("NIDUS_TEST_NATS_URL").expect("server URL is required");
    let provider = NatsProvider::builder(
        NatsConfig::new(url, "nidus-live-test").allow_plaintext_for_local_development(),
    )
    .connect()
    .await
    .unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let stream_name = format!("NIDUS_{suffix}").to_ascii_uppercase();
    let subject = format!("nidus.{suffix}.events");
    let durable = format!("worker-{suffix}");

    let stream = provider
        .jetstream()
        .create_stream(jetstream::stream::Config {
            name: stream_name.clone(),
            subjects: vec![subject.clone()],
            storage: jetstream::stream::StorageType::File,
            ..Default::default()
        })
        .await
        .unwrap();
    let consumer: PullConsumer = stream
        .create_consumer(jetstream::consumer::pull::Config {
            durable_name: Some(durable),
            ack_policy: jetstream::consumer::AckPolicy::Explicit,
            ..Default::default()
        })
        .await
        .unwrap();

    let envelope = MessageEnvelope::new("orders.created", json!({"order_id": 9})).unwrap();
    let ack = provider
        .publish_envelope(&subject, &envelope)
        .await
        .unwrap();
    assert_eq!(ack.stream, stream_name);

    let mut messages = consumer.messages().await.unwrap();
    let message = tokio::time::timeout(Duration::from_secs(10), messages.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let decoded: MessageEnvelope<serde_json::Value> =
        MessageEnvelope::from_json(&message.payload).unwrap();
    assert_eq!(decoded.id(), envelope.id());
    message.ack().await.unwrap();

    provider
        .jetstream()
        .delete_stream(&stream_name)
        .await
        .unwrap();
    provider.shutdown().await.unwrap();
    assert!(matches!(
        provider.publish(&subject, "after-shutdown").await,
        Err(nidus_nats::NatsError::ShuttingDown)
    ));
    provider.shutdown().await.unwrap();
}

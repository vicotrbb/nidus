use std::collections::HashMap;

use aws_sdk_sqs::{
    Client,
    config::{Credentials, Region},
    types::QueueAttributeName,
};
use nidus_integrations::MessageEnvelope;
use nidus_sqs::{SqsConfig, SqsProvider};
use serde_json::json;

#[tokio::test]
#[ignore = "run through scripts/test-integration-services.sh"]
async fn real_sqs_emulator_dlq_send_receive_delete_and_cleanup() {
    let endpoint = std::env::var("NIDUS_TEST_SQS_ENDPOINT").expect("endpoint is required");
    let sdk_config = aws_sdk_sqs::Config::builder()
        .behavior_version_latest()
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new(
            "nidus-test",
            "nidus-test",
            None,
            None,
            "nidus-live-test",
        ))
        .endpoint_url(&endpoint)
        .build();
    let client = Client::from_conf(sdk_config);
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let dlq_name = format!("nidus-dlq-{suffix}");
    let queue_name = format!("nidus-queue-{suffix}");

    let dlq_output = client
        .create_queue()
        .queue_name(&dlq_name)
        .send()
        .await
        .unwrap();
    let dlq_url = normalize_queue_url(&endpoint, dlq_output.queue_url().unwrap());
    let dlq_attributes = client
        .get_queue_attributes()
        .queue_url(&dlq_url)
        .attribute_names(QueueAttributeName::QueueArn)
        .send()
        .await
        .unwrap();
    let dlq_arn = dlq_attributes
        .attributes()
        .unwrap()
        .get(&QueueAttributeName::QueueArn)
        .unwrap();
    let redrive_policy = json!({"deadLetterTargetArn": dlq_arn, "maxReceiveCount": "2"});
    let queue_output = client
        .create_queue()
        .queue_name(&queue_name)
        .set_attributes(Some(HashMap::from([(
            QueueAttributeName::RedrivePolicy,
            redrive_policy.to_string(),
        )])))
        .send()
        .await
        .unwrap();
    let queue_url = normalize_queue_url(&endpoint, queue_output.queue_url().unwrap());

    let provider = SqsProvider::builder_with_client(
        SqsConfig::new(&queue_url)
            .allow_http_for_local_development()
            .with_wait_time_seconds(2)
            .with_max_messages(1),
        client.clone(),
    )
    .connect()
    .await
    .unwrap();
    let envelope = MessageEnvelope::new("orders.created", json!({"order_id": 13})).unwrap();
    let receipt = provider.send_envelope(&envelope).await.unwrap();
    assert!(receipt.message_id().is_some());
    let messages = provider.receive().await.unwrap();
    assert_eq!(messages.len(), 1);
    let decoded: MessageEnvelope<serde_json::Value> =
        serde_json::from_str(messages[0].body().unwrap()).unwrap();
    assert_eq!(decoded.id(), envelope.id());
    provider
        .change_visibility(messages[0].receipt_handle().unwrap(), 30)
        .await
        .unwrap();
    provider
        .delete(messages[0].receipt_handle().unwrap())
        .await
        .unwrap();
    provider.shutdown().await.unwrap();
    assert!(matches!(
        provider.send("after-shutdown").await,
        Err(nidus_sqs::SqsError::ShuttingDown)
    ));
    provider.shutdown().await.unwrap();

    client
        .delete_queue()
        .queue_url(&queue_url)
        .send()
        .await
        .unwrap();
    client
        .delete_queue()
        .queue_url(&dlq_url)
        .send()
        .await
        .unwrap();
}

fn normalize_queue_url(endpoint: &str, returned: &str) -> String {
    returned
        .find("/000000000000/")
        .map(|index| format!("{}{}", endpoint.trim_end_matches('/'), &returned[index..]))
        .unwrap_or_else(|| returned.to_owned())
}

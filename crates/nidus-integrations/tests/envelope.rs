use nidus_integrations::{
    DEFAULT_MAX_ENVELOPE_BYTES, EnvelopeMetadata, IntegrationError, MessageEnvelope,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
struct UserCreated {
    email: String,
}

#[test]
fn envelope_round_trips_without_exposing_payload_or_header_values_in_debug() {
    let metadata = EnvelopeMetadata::new()
        .correlation_id("request-42")
        .unwrap()
        .traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
        .unwrap()
        .header("authorization", "Bearer secret")
        .unwrap();
    let envelope = MessageEnvelope::new(
        "user.created",
        UserCreated {
            email: "ada@example.com".to_owned(),
        },
    )
    .unwrap()
    .with_metadata(metadata);

    let debug = format!("{envelope:?}");
    assert!(!debug.contains("ada@example.com"));
    assert!(!debug.contains("Bearer secret"));
    assert!(debug.contains("authorization"));

    let encoded = envelope.to_json().unwrap();
    let decoded = MessageEnvelope::<UserCreated>::from_json(&encoded).unwrap();
    assert_eq!(decoded, envelope);
}

#[test]
fn envelope_rejects_invalid_trace_context_headers_and_oversized_payloads() {
    assert!(matches!(
        EnvelopeMetadata::new().traceparent("not-a-traceparent"),
        Err(IntegrationError::InvalidTraceparent)
    ));
    assert!(matches!(
        EnvelopeMetadata::new().header("bad header", "value"),
        Err(IntegrationError::InvalidHeaderName)
    ));

    let envelope = MessageEnvelope::new("large", vec![0_u8; DEFAULT_MAX_ENVELOPE_BYTES]).unwrap();
    assert!(matches!(
        envelope.to_json(),
        Err(IntegrationError::EnvelopeTooLarge { .. })
    ));
}

#[test]
fn explicit_envelope_parts_are_validated_after_deserialization() {
    let invalid = br#"{
        "id":"",
        "name":"event",
        "schema_version":1,
        "occurred_at_ms":0,
        "metadata":{"correlation_id":null,"causation_id":null,"traceparent":null,"headers":{}},
        "payload":{}
    }"#;
    assert!(matches!(
        MessageEnvelope::<serde_json::Value>::from_json(invalid),
        Err(IntegrationError::InvalidCorrelationId)
    ));
}

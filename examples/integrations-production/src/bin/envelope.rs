use nidus_integrations::{EnvelopeMetadata, MessageEnvelope};
use serde_json::json;

fn main() -> anyhow::Result<()> {
    let metadata = EnvelopeMetadata::new()
        .correlation_id("request-42")?
        .traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")?;
    let envelope =
        MessageEnvelope::new("orders.created", json!({"order_id": 42}))?.with_metadata(metadata);
    println!("{}", String::from_utf8(envelope.to_json()?)?);
    Ok(())
}

#![cfg(feature = "otel")]

use http::HeaderValue;
use nidus_http::otel::{OtelConfig, TraceContext, extract_trace_context, inject_trace_context};

#[test]
fn otel_config_carries_otlp_endpoint_and_resource_attributes() {
    let config = OtelConfig::new("users-api")
        .version("1.2.3")
        .environment("test")
        .with_otlp_endpoint("http://collector:4317")
        .resource_attribute("region", "local");

    assert_eq!(config.service_name(), "users-api");
    assert_eq!(config.otlp_endpoint(), Some("http://collector:4317"));
    assert_eq!(config.resource_attributes()["service.name"], "users-api");
    assert_eq!(config.resource_attributes()["service.version"], "1.2.3");
    assert_eq!(
        config.resource_attributes()["deployment.environment"],
        "test"
    );
    assert_eq!(config.resource_attributes()["region"], "local");
}

#[test]
fn otel_trace_context_extracts_and_injects_traceparent_headers() {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "traceparent",
        HeaderValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );

    let context = extract_trace_context(&headers).unwrap();
    assert_eq!(context.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(context.span_id(), "00f067aa0ba902b7");
    assert!(context.sampled());

    let mut injected = http::HeaderMap::new();
    inject_trace_context(&mut injected, &context);
    assert_eq!(
        injected.get("traceparent").unwrap(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    );

    assert!(TraceContext::parse("not-a-traceparent").is_none());
}

#[test]
fn otel_trace_context_rejects_invalid_w3c_identifiers_and_versions() {
    for value in [
        "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
        "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
    ] {
        assert!(TraceContext::parse(value).is_none(), "{value}");
    }
}

#[test]
fn otel_trace_context_accepts_and_ignores_future_version_extensions() {
    let context =
        TraceContext::parse("01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-03-vendor-data")
            .unwrap();

    assert_eq!(context.trace_id(), "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(context.span_id(), "00f067aa0ba902b7");
    assert!(context.sampled());
}

# Release 1.0.10

Nidus 1.0.10 adds a first-party integration ecosystem without adding vendor
clients to the `nidus-rs` facade. Existing 1.0 APIs remain available, and each
new adapter is installed separately.

## Architecture

- `nidus-integrations` owns only bounded envelopes, correlation, lifecycle
  state, health, and redaction-safe telemetry composition.
- Redis, Kafka, NATS/JetStream, RabbitMQ, SQS, OpenTelemetry, Sentry, and
  SQLx-backed durable jobs are separate crates that expose their native clients.
- Kafka partitions and offsets, JetStream consumers, RabbitMQ topology, and
  SQS visibility receipts remain backend-specific APIs rather than a generic
  queue facade.

## Data and durable work

- SQLx MySQL uses rustls/native roots and requires `VERIFY_IDENTITY` outside an
  explicit loopback development connection.
- CockroachDB v26.2.0 is tested with generated certificates and
  `sslmode=verify-full`. Transaction retries are bounded, use full jitter, and
  retry only SQLSTATE `40001`; ambiguous `40003` results are returned.
- Durable jobs are persisted and at-least-once. They include scheduling,
  enqueue idempotency, attempt-fenced leases, heartbeats, retries, cancellation,
  recovery, acknowledgements, indexed DLQs, and bounded graceful shutdown.

## Messaging and telemetry

- Kafka uses delivery reports, idempotent producer defaults, manual consumer
  commits, and `read_committed` isolation.
- Core NATS remains at-most-once; JetStream publishing waits for persistence
  acknowledgement. RabbitMQ publishing uses mandatory persistent messages and
  broker confirms. Standard SQS remains at-least-once.
- OpenTelemetry uses the official SDK, bounded batching, W3C propagation,
  OTLP/gRPC or OTLP/HTTP over rustls, and explicit flush/shutdown.
- Sentry owns initialization and restoration, isolated Tower request hubs,
  matched-route transactions, tracing/error/panic integrations, redaction,
  deduplication, and graceful flushing.

## Verification boundary

The release candidate is checked with strict workspace formatting, Clippy,
tests, rustdoc, isolated feature builds, real pinned service tests, dependency
policy and RustSec audits, semver checks against 1.0.9, package file-list
preflights, and Criterion integration hot-path comparisons. The real-service
harness proves cleanup on success, failure, injected Rust panic, and
interruption.

After publishing the crates in dependency order, verify registry artifacts and
docs.rs pages with:

```bash
bash scripts/verify-published-release.sh 1.0.10
```

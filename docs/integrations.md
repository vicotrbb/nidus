# First-party integrations

Nidus integrations are optional adapter crates. The `nidus-rs` facade and core
runtime do not depend on Redis, SQLx, a message broker, an observability SDK, or
a vendor error reporter unless an application installs that adapter.

## Contract

Every first-party adapter provides the parts Nidus can own safely:

- validated, redaction-safe configuration and secure transport defaults;
- typed singleton registration through `nidus_core::Container`;
- lifecycle hooks for clients that must drain or flush;
- health/readiness checks where the backend provides a meaningful probe;
- bounded adapter-owned concurrency and backpressure;
- shared tracing, metrics, and dashboard events through
  `nidus_integrations::IntegrationTelemetry`;
- the native ecosystem client for backend-specific capabilities and errors.

`nidus-integrations` supplies only stable cross-backend concepts:
`MessageEnvelope<T>`, bounded correlation/causation/trace metadata, lifecycle
status, and telemetry observers. It deliberately does not define a generic
queue trait. Kafka partitions and offsets, JetStream streams and consumers,
RabbitMQ exchanges and acknowledgements, and SQS visibility receipts remain
native APIs.

```rust
let telemetry = nidus_integrations::IntegrationTelemetry::new()
    .tracing()
    .observability(observability.adapter_observer())
    .dashboard(dashboard.clone());
```

## Adapter matrix

| Crate | Native capability exposed | Nidus-owned behavior |
| --- | --- | --- |
| `nidus-redis` | `redis::Client`, reconnecting `ConnectionManager` | bounded reconnect/backoff, get/set/delete helpers, health, drain admission |
| `nidus-sqlx` | `SqlitePool`, `PgPool`, `MySqlPool` | typed pools, MySQL validation, CockroachDB retry helper |
| `nidus-kafka` | rust-rdkafka producer, admin, stream consumer | delivery reports, manual commit defaults, flush, health |
| `nidus-nats` | async-nats Core client and JetStream context | bounded publish, persistence acknowledgement, drain, health |
| `nidus-rabbitmq` | Lapin connection and channels | confirms, mandatory persistent publish, QoS, close, health |
| `nidus-sqs` | official AWS SDK SQS client and messages | long poll, visibility changes, delete, FIFO send, health, request drain |
| `nidus-jobs-sqlx` | `sqlx::AnyPool` | durable schema, leases, retries, recovery, cancellation, DLQ |
| `nidus-opentelemetry` | SDK tracer/provider and tracing layer | OTLP gRPC/HTTP, propagation, bounded batching, flush |
| `nidus-sentry` | Sentry client and request-local hubs | Tower isolation, matched routes, redaction, dedupe, flush |

Install only the adapters in use:

```toml
nidus = { package = "nidus-rs", version = "1.0.13", features = ["http", "config"] }
nidus-integrations = "1.0.13"
nidus-redis = { version = "1.0.13", features = ["health", "observability"] }
nidus-kafka = { version = "1.0.13", features = ["health", "observability"] }
```

`nidus-kafka` compiles librdkafka from source. Linux build images must provide
CMake, a C/C++ toolchain, `pkg-config`, and libcurl development headers (for
example, `libcurl4-openssl-dev` on Debian or Ubuntu). TLS itself uses vendored
OpenSSL by default. The Nidus CI and release workflows install and validate
these prerequisites on clean Ubuntu runners.

## Delivery semantics

- Kafka producer idempotence and `acks=all` reduce duplicate writes by one
  producer session. They do not make processing plus an external side effect
  exactly once. Consumers are manual-commit and `read_committed` by default.
- Core NATS is at-most-once. JetStream publish waits for a persistence
  acknowledgement, while consumers must acknowledge only after successful,
  idempotent processing.
- RabbitMQ publish waits for a positive broker confirmation and uses mandatory
  routing and persistent messages. Consumers still own acknowledgement,
  redelivery, topology, and dead-letter exchange policy.
- Standard SQS queues are at-least-once and may deliver duplicates. Delete the
  receipt only after success; extend visibility for long work. FIFO ordering
  and deduplication do not make arbitrary side effects exactly once.
- Durable Nidus jobs are at-least-once. A worker can crash after a side effect
  and before acknowledgement, so handlers must be idempotent.

## Secure defaults

Production constructors require encrypted endpoints: `rediss://`, Kafka SSL,
`tls://` or `wss://` NATS, `amqps://`, HTTPS SQS, HTTPS Sentry, HTTPS OTLP,
CockroachDB `sslmode=verify-full`, and MySQL
`ssl-mode=VERIFY_IDENTITY`. Plaintext escape hatches are explicit and reject
non-loopback hosts. Configuration `Debug` output redacts URLs, DSNs, and header
values.

The native clients remain responsible for credentials, certificate selection,
broker ACLs, AWS IAM, and backend-specific reconnect policy. Prefer workload
identity or secret stores and never place secrets in telemetry labels,
envelopes, job error strings, or dashboard metadata.

Adapter-owned operations stop admission before lifecycle shutdown and drain
within configured deadlines. Kafka flushes its native producer, NATS drains the
connection, RabbitMQ closes confirm channels and the connection, Redis waits
for admitted commands, and SQS waits for admitted SDK requests. Native client
clones are deliberately exposed and remain application-owned; calls made
directly through them are outside Nidus admission and shutdown accounting.

## Validation and live examples

Every adapter has deterministic configuration/failure tests. The pinned live
suite creates unique service resources, exercises real protocols, deletes
topics/queues/streams/tables, and verifies that its containers, networks,
volumes, temporary paths, ports, and worktree diff return to the initial state:

```bash
bash scripts/check-integration-feature-matrix.sh
bash scripts/test-integration-services.sh
```

Runnable per-adapter binaries and required environment variables are listed in
`examples/integrations-production/README.md`.

## Primary upstream references

- [redis-rs](https://docs.rs/redis/latest/redis/)
- [SQLx 0.8.6](https://docs.rs/sqlx/0.8.6/sqlx/) (the Nidus 1.x compatibility line; upstream 0.9.0 was also reviewed)
- [rust-rdkafka](https://docs.rs/rdkafka/latest/rdkafka/)
- [async-nats JetStream](https://docs.rs/async-nats/latest/async_nats/jetstream/)
- [Lapin](https://docs.rs/lapin/latest/lapin/)
- [AWS SDK for Rust SQS](https://docs.rs/aws-sdk-sqs/latest/aws_sdk_sqs/)
- [OpenTelemetry OTLP](https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/)
- [Sentry Tower](https://docs.rs/sentry-tower/latest/sentry_tower/)

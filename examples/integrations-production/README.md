# Production integration examples

Each binary demonstrates one separately installable first-party adapter while
retaining native client access. Secure transport is required by default; the
`NIDUS_ALLOW_LOCAL_PLAINTEXT=1` escape hatch works only with loopback URLs.

The Kafka binary builds librdkafka from source. On Debian or Ubuntu build
images, install CMake, a C/C++ toolchain, `pkg-config`, and
`libcurl4-openssl-dev` before compiling it.

```bash
cargo run -p nidus-example-integrations-production --bin envelope
REDIS_URL=rediss://redis.example.test cargo run -p nidus-example-integrations-production --bin redis
MYSQL_DATABASE_URL='mysql://app:secret@db.example.test/app?ssl-mode=VERIFY_IDENTITY' cargo run -p nidus-example-integrations-production --bin mysql
COCKROACH_DATABASE_URL='postgresql://app@db.example.test/app?sslmode=verify-full&sslrootcert=/path/ca.crt' cargo run -p nidus-example-integrations-production --bin cockroach
KAFKA_BOOTSTRAP_SERVERS='broker.example.test:9093' cargo run -p nidus-example-integrations-production --bin kafka
NATS_URL='tls://nats.example.test:4222' cargo run -p nidus-example-integrations-production --bin nats
RABBITMQ_URL='amqps://app:secret@rabbit.example.test/%2f' cargo run -p nidus-example-integrations-production --bin rabbitmq
SQS_QUEUE_URL='https://sqs.us-east-1.amazonaws.com/123456789012/jobs' cargo run -p nidus-example-integrations-production --bin sqs
cargo run -p nidus-example-integrations-production --bin durable-jobs
OTEL_EXPORTER_OTLP_ENDPOINT='https://collector.example.test:4317' cargo run -p nidus-example-integrations-production --bin opentelemetry
SENTRY_DSN='https://public@example.ingest.sentry.io/1' cargo run -p nidus-example-integrations-production --bin sentry
```

The service-backed behavior, acknowledgement paths, retries, DLQs, TLS, and
resource deletion are exercised by `scripts/test-integration-services.sh`.

# Official adapters

Official adapters are separately installable crates. The facade stays lean,
and vendor dependencies enter the build only when selected.

| Area | Crates |
| --- | --- |
| Data | `nidus-redis`, `nidus-sqlx`, `nidus-cache` |
| Messaging | `nidus-kafka`, `nidus-nats`, `nidus-rabbitmq`, `nidus-sqs` |
| Durable jobs | `nidus-jobs`, `nidus-jobs-sqlx` |
| Telemetry and errors | `nidus-opentelemetry`, `nidus-sentry` |
| Shared composition | `nidus-integrations` |

Adapters register typed providers, integrate with configuration, lifecycle,
health/readiness, observability, and dashboard events, and expose their native
clients. There is no lowest-common-denominator message queue API.

```toml
nidus-redis = { version = "1.0.13", features = ["health", "observability"] }
nidus-jobs-sqlx = { version = "1.0.13", features = ["postgres", "observability"] }
nidus-opentelemetry = "1.0.13"
nidus-sentry = "1.0.13"
```

See [first-party integrations](integrations.md), [SQLx](sqlx.md),
[jobs](jobs.md), [observability](observability.md), and the runnable
`examples/integrations-production` package.

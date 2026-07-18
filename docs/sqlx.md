# SQLx data adapters

`nidus-sqlx` provides typed SQLite, PostgreSQL, MySQL, and CockroachDB pool
providers. It owns configuration, DI, readiness, and adapter telemetry; SQL,
migrations, schema design, and backend-specific operations remain normal SQLx.

```toml
nidus-sqlx = { version = "1.0.13", features = ["mysql", "cockroach", "health", "observability"] }
```

Features are independent: `sqlite`, `postgres`, `mysql`, `cockroach`,
`nidus-config`, `health`, and `observability`. `cockroach` enables SQLx's
PostgreSQL driver and rustls certificate support; `mysql` enables the MySQL
driver with rustls and native trust roots so `VERIFY_IDENTITY` works without a
separate hidden feature.

## SQLx compatibility line

The current upstream SQLx release was verified as 0.9.0 during this work.
Nidus 1.x intentionally remains on SQLx 0.8.6 because the existing public
providers expose concrete SQLx pool types. Moving the workspace to 0.9 would
change those public type identities and the minimum supported Rust version, so
it belongs in a separately reviewed Nidus major release rather than this
compatibility-preserving adapter addition. Applications must use a compatible
SQLx 0.8 dependency when naming the returned pool types directly.

## MySQL

MySQL requires hostname-verifying TLS by default:

```rust
let provider = nidus_sqlx::MySqlPoolProvider::builder(
    "mysql://app:secret@db.example.test/app?ssl-mode=VERIFY_IDENTITY",
)
.max_connections(20)
.connect()
.await?;

let pool: &sqlx::MySqlPool = provider.pool();
```

Only an explicit `allow_insecure_for_local_development()` configuration may
use a non-verifying loopback URL. The integration suite tests MySQL 8.4 with a
real pool, query round trip, schema deletion, and multi-worker job leases.

## CockroachDB compatibility and TLS

CockroachDB uses its PostgreSQL wire protocol but has distinct transaction
retry semantics. `CockroachPoolProvider` requires `sslmode=verify-full` unless
the caller explicitly opts into a loopback-only development connection.

```rust
use nidus_sqlx::{CockroachPoolConfig, CockroachPoolProvider, CockroachRetryPolicy};

let retry = CockroachRetryPolicy::new()
    .with_max_attempts(5)
    .with_backoff(
        std::time::Duration::from_millis(25),
        std::time::Duration::from_secs(2),
    );
let config = CockroachPoolConfig::new(
    "postgresql://app@db.example.test/app?sslmode=verify-full&sslrootcert=/run/secrets/ca.crt",
)
.with_retry_policy(retry);
let provider = CockroachPoolProvider::builder(config.database_url())
    .config(config)
    .connect()
    .await?;
```

The live compatibility target is CockroachDB v26.2.0. The test generates a
throwaway CA, node certificate, and client certificate, connects with hostname
verification, injects serialization failures, proves the retry count, and then
deletes the schema and credentials. This proves the tested target; it is not a
claim that every PostgreSQL extension or every CockroachDB release is
compatible.

### Safe retries

`transaction_with_retry` retries only SQLSTATE `40001` serialization failures,
with bounded exponential full jitter. SQLSTATE `40003` ambiguous-result errors
and all other errors are returned immediately. The callback can run more than
once and therefore must contain only effects covered by that database
transaction. Do not call HTTP services, publish messages, send email, mutate
Redis, or perform another external side effect in the callback.

```rust
let user_id = provider
    .transaction_with_retry(|connection| {
        Box::pin(async move {
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO users (email) VALUES ($1) RETURNING id",
            )
            .bind("ada@example.test")
            .fetch_one(connection)
            .await
        })
    })
    .await?;
```

For database plus messaging, persist an outbox record in the retried
transaction and publish it in a separate idempotent dispatcher.

Primary references: [CockroachDB connection parameters](https://www.cockroachlabs.com/docs/stable/connection-parameters),
[transaction retry errors](https://www.cockroachlabs.com/docs/stable/transaction-retry-error-reference),
and [transactions](https://www.cockroachlabs.com/docs/stable/transactions).

## DI, readiness, and native access

All pool builders can register asynchronously into a Nidus container and their
providers implement the lifecycle hook that closes the native pool. With the
`health` feature, a resolved `Arc<...Provider>` can add a readiness check. With
`observability`, connection, health, and retry operations emit adapter events.
Queries issued directly through `pool()` are native SQLx calls and are not
silently wrapped or rewritten by Nidus.

Primary SQLx API reference: [SQLx 0.8.6](https://docs.rs/sqlx/0.8.6/sqlx/).

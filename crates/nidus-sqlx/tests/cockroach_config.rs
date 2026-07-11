#![cfg(feature = "cockroach")]

use std::time::Duration;

use nidus_sqlx::{CockroachPoolConfig, CockroachPoolProvider, CockroachRetryPolicy, SqlxError};

#[test]
fn cockroach_requires_verify_full_tls_unless_local_insecure_is_explicit() {
    fn assert_lifecycle<T: nidus_core::LifecycleHook>() {}
    assert_lifecycle::<CockroachPoolProvider>();
    let insecure =
        CockroachPoolConfig::new("postgresql://root@localhost:26257/defaultdb?sslmode=disable");
    assert!(matches!(
        insecure.validate(),
        Err(SqlxError::Sqlx(sqlx::Error::InvalidArgument(_)))
    ));
    assert!(
        insecure
            .clone()
            .allow_insecure_for_local_development()
            .validate()
            .is_ok()
    );

    let remote_insecure =
        CockroachPoolConfig::new("postgresql://root@db.example:26257/defaultdb?sslmode=disable")
            .allow_insecure_for_local_development();
    assert!(matches!(
        remote_insecure.validate(),
        Err(SqlxError::Sqlx(sqlx::Error::InvalidArgument(_)))
    ));

    let secure = CockroachPoolConfig::new(
        "postgresql://app@db.example:26257/defaultdb?sslmode=verify-full&sslrootcert=/tmp/ca.crt",
    );
    assert!(secure.validate().is_ok());
    assert!(!format!("{secure:?}").contains("db.example"));
    assert!(secure.clone().with_max_connections(0).validate().is_err());
}

#[test]
fn retry_policy_is_bounded_exponential_and_validated() {
    let policy = CockroachRetryPolicy::new()
        .with_max_attempts(4)
        .with_backoff(Duration::from_millis(10), Duration::from_millis(25))
        .without_jitter();
    assert_eq!(policy.max_attempts(), 4);
    assert_eq!(policy.maximum_delay_for_retry(1), Duration::from_millis(10));
    assert_eq!(policy.maximum_delay_for_retry(2), Duration::from_millis(20));
    assert_eq!(policy.maximum_delay_for_retry(3), Duration::from_millis(25));

    let invalid =
        CockroachPoolConfig::new("postgresql://app@db.example:26257/defaultdb?sslmode=verify-full")
            .with_retry_policy(policy.with_max_attempts(0));
    assert!(matches!(
        invalid.validate(),
        Err(SqlxError::Sqlx(sqlx::Error::InvalidArgument(_)))
    ));
}

use nidus_jobs::DurableJobError;
use nidus_jobs_sqlx::{SqlDialect, SqlxJobStore, SqlxJobStoreConfig};

#[test]
fn cockroach_requires_hostname_verified_tls() {
    let secure = SqlxJobStoreConfig::cockroach(
        "postgres://nidus@example.test:26257/app?sslmode=verify-full",
    );
    assert_eq!(secure.dialect(), SqlDialect::Cockroach);

    let insecure =
        SqlxJobStoreConfig::cockroach("postgres://root@localhost:26257/defaultdb?sslmode=disable")
            .allow_insecure_local_cockroach();
    assert!(insecure.is_ok());

    let remote_insecure = SqlxJobStoreConfig::cockroach(
        "postgres://root@example.test:26257/defaultdb?sslmode=disable",
    )
    .allow_insecure_local_cockroach();
    assert!(remote_insecure.is_err());
}

#[test]
fn debug_output_redacts_database_credentials() {
    let config = SqlxJobStoreConfig::mysql("mysql://alice:secret@example.test/app");
    let debug = format!("{config:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("alice"));
}

#[tokio::test]
async fn local_escape_hatches_parse_actual_hosts_and_unambiguous_tls_modes() {
    assert!(
        SqlxJobStoreConfig::mysql("mysql://root@127.0.0.1:13306/app")
            .allow_insecure_local_mysql()
            .is_ok()
    );
    assert!(
        SqlxJobStoreConfig::cockroach("postgresql://root@127.0.0.1:26257/app?sslmode=disable",)
            .allow_insecure_local_cockroach()
            .is_ok()
    );
    assert!(
        SqlxJobStoreConfig::cockroach(
            "postgresql://localhost:26257@evil.example:26257/app?sslmode=disable",
        )
        .allow_insecure_local_cockroach()
        .is_err()
    );
    assert!(
        SqlxJobStoreConfig::mysql("mysql://localhost:3306@evil.example:3306/app")
            .allow_insecure_local_mysql()
            .is_err()
    );
    assert!(
        SqlxJobStoreConfig::mysql("postgresql://localhost:5432/app")
            .allow_insecure_local_mysql()
            .is_err()
    );
    assert!(
        SqlxJobStoreConfig::postgres("postgresql://localhost:5432/app?sslmode=disable")
            .allow_insecure_local_postgres()
            .is_ok()
    );
    assert!(
        SqlxJobStoreConfig::postgres("postgresql://db.example/app?sslmode=disable")
            .allow_insecure_local_postgres()
            .is_err()
    );

    let cockroach = SqlxJobStore::connect(SqlxJobStoreConfig::cockroach(
        "postgresql://db.example/app?sslmode=verify-full&sslmode=disable",
    ))
    .await
    .unwrap_err();
    assert!(matches!(cockroach, DurableJobError::Configuration(_)));

    let mysql = SqlxJobStore::connect(SqlxJobStoreConfig::mysql(
        "mysql://db.example/app?ssl-mode=VERIFY_IDENTITY&ssl-mode=DISABLED",
    ))
    .await
    .unwrap_err();
    assert!(matches!(mysql, DurableJobError::Configuration(_)));
}

# Nidus Dashboard

`nidus-dashboard` is an optional embedded dashboard for inspecting a running
Nidus application from the same server process as the app.

```toml
nidus = { package = "nidus-rs", version = "1.0.4", features = ["dashboard"] }
```

```rust
let dashboard = NidusDashboard::builder()
    .path("/nidus/dashboard")
    .auth(DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN"))
    .storage(DashboardStorage::sqlite_from_env("NIDUS_DASHBOARD_DATABASE_URL"))
    .capture(DashboardCapture::metadata_only())
    .retention(DashboardRetention::days(7).max_events(100_000))
    .build()?;

let app = Nidus::create::<AppModule>()
    .with_dashboard(dashboard)
    .build()
    .await?;
```

Dashboard routes are protected by default. Building without dashboard auth
fails, missing bearer tokens fail, and invalid bearer tokens fail. Use
`DashboardAuth::unsafe_disabled_for_local_development()` only for local
development.

Default capture is metadata-only. Payload capture is opt-in, byte-capped, and
redacted before storage.

The embedded routes are:

- `/nidus/dashboard/`
- `/nidus/dashboard/assets/styles.css`
- `/nidus/dashboard/assets/app.js`
- `/nidus/dashboard/api/overview`
- `/nidus/dashboard/api/routes`
- `/nidus/dashboard/api/settings`
- `/nidus/dashboard/api/timeline`
- `/nidus/dashboard/stream`

See `examples/dashboard-api` for a runnable app with bearer auth, SQLite
storage, route snapshots, event capture, job capture, dashboard APIs, and SSE
checks.

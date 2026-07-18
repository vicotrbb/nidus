# Nidus Dashboard

`nidus-dashboard` is an optional embedded runtime cockpit for inspecting a
running Nidus application from the same server process as the app. It is not
enabled by default.

Enable it through the facade:

```toml
nidus = { package = "nidus-rs", version = "1.0.13", features = ["dashboard"] }
```

Or depend on the crate directly when building lower-level integration code:

```toml
nidus-dashboard = "1.0.13"
```

## Setup

```rust
use nidus::prelude::*;

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
`DashboardAuth::unsafe_disabled_for_local_development()` only for a trusted
local demo or test server.

## Runtime Cockpit

The embedded UI is organized around the current runtime model:

- **Home:** service, connection, storage, capture, auth, application shape,
  timeline health, recorded operation durations, and the latest five signals.
- **Atlas:** module graph topology, route topology for the selected module, and
  activity signals linked to graph nodes.
- **Routes:** recorded HTTP route snapshots.
- **Timeline:** unified runtime records with segmented filters for **All**,
  **Events**, and **Jobs**.
- **Adapters:** official adapter operations when adapter hooks record them.
- **Settings:** effective auth, capture, storage, and retention settings.

Events and Jobs no longer have standalone dashboard navigation pages. Their
data collection and JSON APIs remain available, and the UI consolidates those
records into Timeline.

## Modes

Auth modes:

- `bearer` -> Bearer token
- `unsafe_disabled_for_local_development` -> Auth disabled locally

Capture modes:

- `metadata_only` -> Metadata only
- `payloads_redacted` -> Payloads redacted

Storage modes:

- `memory` -> Memory
- `sqlite` -> SQLite

Default capture is metadata-only. Payload capture is opt-in, byte-capped, and
redacted before storage.

## Routes

With the default mount path, the embedded routes are:

- `/nidus/dashboard/`
- `/nidus/dashboard/assets/styles.css`
- `/nidus/dashboard/assets/app.js`
- `/nidus/dashboard/assets/logo-mark-square-transparent.png`
- `/nidus/dashboard/assets/favicon-branded-32.png`
- `/nidus/dashboard/assets/favicon-branded-192.png`
- `/nidus/dashboard/assets/apple-touch-icon.png`
- `/nidus/dashboard/api/overview`
- `/nidus/dashboard/api/graph`
- `/nidus/dashboard/api/routes`
- `/nidus/dashboard/api/events`
- `/nidus/dashboard/api/jobs`
- `/nidus/dashboard/api/adapters`
- `/nidus/dashboard/api/settings`
- `/nidus/dashboard/api/timeline`
- `/nidus/dashboard/stream`

The auth middleware covers the dashboard shell, embedded assets, JSON APIs, and
SSE stream.

## Local Runs

Bearer-auth demo:

```bash
export NIDUS_DASHBOARD_TOKEN=dev-dashboard-token
export NIDUS_DASHBOARD_DATABASE_URL='sqlite://./target/nidus-dashboard-example.sqlite?mode=rwc'
cargo run -p nidus-example-dashboard-api
```

Auth-disabled local demo:

```bash
export NIDUS_DASHBOARD_DISABLE_AUTH=1
export NIDUS_DASHBOARD_DATABASE_URL='sqlite://./target/nidus-dashboard-example.sqlite?mode=rwc'
cargo run -p nidus-example-dashboard-api
```

Open the UI at:

```text
http://127.0.0.1:4310/nidus/dashboard/
```

See `examples/dashboard-api` for a runnable app with SQLite storage, route
snapshots, event capture, job capture, graph APIs, Timeline proof commands, and
SSE checks.

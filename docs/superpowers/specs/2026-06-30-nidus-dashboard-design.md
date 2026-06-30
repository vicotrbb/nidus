# Nidus Dashboard Design

Date: 2026-06-30
Status: Approved for implementation planning

## Summary

Nidus Dashboard is an optional embedded dashboard crate for Nidus applications. It lets application owners inspect live framework behavior from inside the running app without deploying a separate collector or dashboard process.

The first version is an opt-in crate named `nidus-dashboard`. It mounts a protected dashboard UI, dashboard JSON APIs, and a live event stream under the host Nidus/Axum application, defaulting to `/nidus/dashboard`. It is production-capable when configured with authentication, bounded retention, and metadata-first capture.

Nidus Dashboard is an introspection product, not a mutation console. Version 1 does not trigger jobs, publish events, replay requests, or call application routes.

## Goals

- Provide a native Nidus dashboard experience comparable in usefulness to NestJS Devtools, but designed around Nidus concepts.
- Ship as a separate optional crate, disabled unless the user installs and mounts it.
- Serve the dashboard UI and APIs directly from the Nidus application.
- Support production-capable operation with built-in zero-trust dashboard authentication.
- Persist lightweight dashboard data to SQLite by default, while allowing user configuration.
- Capture metadata by default and make payload/body capture explicit, bounded, and redacted.
- Reuse the existing Nidus website style and palette, with `$impeccable` as the quality bar for all UI work.

## Non-Goals

- No separate dashboard server or sidecar in v1.
- No request replay, route invocation, event publishing, or job triggering in v1.
- No durable distributed telemetry backend. SQLite is a local dashboard store, not a replacement for Prometheus, OpenTelemetry, or Grafana.
- No automatic public exposure. Mounting must fail closed unless dashboard authentication or an explicit unsafe development override is configured.
- No broad framework rewrite. The dashboard must compose with existing Nidus observability, event, job, route, and module metadata surfaces.

## Users And Context

Primary users are senior Rust backend engineers, framework evaluators, and service owners running Nidus applications. They need to answer practical questions while developing or operating a service:

- What modules, routes, providers, jobs, and event flows exist in this app?
- What is happening right now?
- Which routes or jobs are failing?
- Which events are being published?
- Which official adapters are active and how are they behaving?
- Is the dashboard capture configuration safe for this environment?

The interface should feel sharp, deliberate, inspectable, and consistent with the current Nidus website.

## Package Shape

Add a new optional crate:

```text
crates/nidus-dashboard/
```

Expected package name:

```toml
nidus-dashboard = "1.0.x"
```

The Nidus facade may expose a feature-gated integration after the standalone crate is in place:

```toml
nidus = { package = "nidus-rs", version = "1.0.x", features = ["dashboard"] }
```

The standalone crate remains usable directly with Axum routers.

## Mounting Model

The first version is embedded in the application server:

```text
Nidus App
  |
  +-- user routes
  +-- /metrics
  +-- /openapi.json
  +-- /nidus/dashboard
  +-- /nidus/dashboard/assets/*
  +-- /nidus/dashboard/api/*
  +-- /nidus/dashboard/stream
```

Default path:

```text
/nidus/dashboard
```

The path must be configurable.

## Public Rust API

Required ergonomic setup:

```rust
let dashboard = NidusDashboard::builder()
    .path("/nidus/dashboard")
    .storage(DashboardStorage::sqlite_from_env("NIDUS_DASHBOARD_DATABASE_URL"))
    .auth(DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN"))
    .capture(DashboardCapture::metadata_only())
    .retention(DashboardRetention::days(7).max_events(100_000))
    .build()?;

let app = Nidus::create::<AppModule>()
    .with_observability(observability.clone())
    .with_dashboard(dashboard)
    .build()
    .await?;
```

The direct Axum shape must also be available:

```rust
let dashboard = NidusDashboard::builder()
    .auth(DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN"))
    .build()?;

let router = app_router.merge(dashboard.router());
```

## Configuration

Key configuration types:

```rust
NidusDashboard
DashboardStorage
DashboardAuth
DashboardCapture
DashboardRetention
DashboardRedaction
```

Storage options:

```rust
DashboardStorage::sqlite("nidus-dashboard.sqlite")
DashboardStorage::sqlite_from_env("NIDUS_DASHBOARD_DATABASE_URL")
DashboardStorage::memory()
```

Future storage extension point:

```rust
DashboardStorage::custom(...)
```

Capture options:

```rust
DashboardCapture::metadata_only()
DashboardCapture::payloads()
    .redact_headers(["authorization", "cookie", "x-api-key"])
    .redact_fields(["password", "token", "secret"])
    .max_payload_bytes(16 * 1024)
```

## Security Model

Nidus Dashboard must use a zero-trust default.

Required v1 behavior:

- Mounting without auth fails.
- The built-in auth path supports bearer token authentication.
- Tokens can be loaded from environment variables.
- Invalid or missing credentials return an authentication failure before serving UI assets, JSON APIs, or streams.
- An explicit unsafe development override may exist, but must be noisy in naming and documentation.
- Users can wrap the dashboard router with their own Axum layer or Nidus guard/RBAC.

Suggested API:

```rust
DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN")
DashboardAuth::bearer_token(...)
DashboardAuth::custom_layer(...)
DashboardAuth::unsafe_disabled_for_local_development()
```

The dashboard must not store raw auth tokens in SQLite.

## Storage Model

SQLite is the default storage backend. It must use:

- Automatic lightweight migrations.
- WAL mode when supported.
- Bounded retention.
- Pruning by age and max row count.
- Simple tables optimized for timeline queries and per-surface summaries.

In-memory storage remains useful for tests, demos, and ephemeral environments.

Initial logical records:

- `dashboard_operations`: unified HTTP/event/job/lifecycle/adapter timeline records.
- `dashboard_routes`: route metadata snapshots.
- `dashboard_graph_nodes`: module/provider/controller graph nodes.
- `dashboard_graph_edges`: graph edges.
- `dashboard_settings`: schema version and local dashboard metadata.

The implementation plan can refine table names, but the storage must preserve the product model: unified timeline plus indexed summaries.

## Capture Model

Metadata-only capture is the default.

Always safe metadata:

- timestamp
- surface kind: HTTP, event, job, lifecycle, adapter
- operation ID
- trace ID or request ID when available
- route template, method, status, and duration for HTTP
- event name and attributes for observed events
- job name, run ID, status, and duration for jobs
- adapter name, operation name, status, and duration for official adapters
- error class or status category

Payloads, request bodies, response bodies, raw headers, SQL text, cache keys, and tenant-controlled labels must not be captured by default.

Payload capture, if enabled, must be:

- opt-in
- byte-capped
- redacted
- visible in dashboard settings
- documented as sensitive

## Data Flow

```text
HTTP metrics/tracing hooks
ObservedEventBus
ObservedJobRunner
adapter observers
module graph / route metadata
        |
        v
Dashboard collector
        |
        +--> SQLite retention store
        +--> in-memory live ring buffer
        +--> SSE stream
        |
        v
Embedded dashboard UI
```

The collector must not block request, event, or job paths on slow dashboard writes. Use bounded channels or best-effort enqueue semantics where needed. Dropped dashboard telemetry must not break application behavior.

## Dashboard APIs

Initial API shape:

```text
GET /nidus/dashboard/api/overview
GET /nidus/dashboard/api/graph
GET /nidus/dashboard/api/routes
GET /nidus/dashboard/api/timeline
GET /nidus/dashboard/api/events
GET /nidus/dashboard/api/jobs
GET /nidus/dashboard/api/adapters
GET /nidus/dashboard/api/settings
GET /nidus/dashboard/stream
```

`/stream` must use SSE in v1. SSE is enough for one-way live introspection and keeps the implementation simpler than WebSockets.

Every dashboard API must pass through the dashboard auth gate.

## Dashboard Views

Overview:

- service name, version, environment
- app health and dashboard storage health
- request volume, error rate, event volume, job runs
- recent failures

Graph:

- modules, providers, controllers, route ownership
- lifecycle status
- searchable graph/table hybrid for dense apps

Routes:

- method, path, handler, guards, middleware metadata when available
- recent status, duration, error counts
- link from route to timeline entries

Timeline:

- unified live stream across HTTP, events, jobs, lifecycle, and adapters
- filters by surface, status, route, event name, job name, adapter, operation ID, and request ID
- inspector panel for selected operation metadata

Events:

- event names
- operation IDs
- recent publications
- attributes
- publication counts

Jobs:

- sync and async job runs
- status
- duration
- failures
- run IDs

Adapters:

- official adapter operations
- SQLx/cache operation status and duration where hooks exist
- recent failures

Settings:

- auth mode
- capture mode
- redaction status
- retention settings
- SQLite path/health
- dashboard package version

## UI Direction

All UI work must strongly use `$impeccable`.

Carry the current Nidus website style and palette into a denser dashboard product surface:

- light-first pearl and violet-tinted surfaces
- dark ink text
- restrained violet identity accents
- dark technical panes only where they improve scanability
- 8px radius or less
- organic nesting motif from the Nidus logo as the memorable structural element
- dense, scannable app UI rather than a marketing page

Existing website tokens to preserve conceptually:

- pearl/violet background
- violet brand accents
- dark code/technical panels
- `Afacad Flux` body type
- `Bricolage Grotesque` display headings

The dashboard must avoid:

- generic neon observability styling
- full-page black backgrounds
- glassy glow-heavy panels
- gradient text
- decorative side-stripe borders
- oversized hero sections
- card-inside-card layouts

Primary layout:

```text
+--------------------------------------------------------------------------------+
| Nidus Dashboard           live  users-api  prod              token auth enabled |
+------------------+--------------------------------+----------------------------+
| Overview         | Timeline                       | Inspector                  |
| Graph            | 12:04:18 GET /users 200 8ms    | kind: event                |
| Routes           | 12:04:19 job digest ok 42ms    | name: user.created         |
| Timeline         | 12:04:20 cache get ok 1ms      | operation_id: ...          |
| Events           | 12:04:21 event user.created    | request_id: ...            |
| Jobs             |                                | attributes                 |
| Adapters         |                                |                            |
| Settings         |                                |                            |
+------------------+--------------------------------+----------------------------+
```

Responsive behavior:

- Desktop: sidebar + main view + inspector.
- Tablet: sidebar collapses to top navigation, inspector becomes a lower panel.
- Mobile: preserve all core functionality with tabbed navigation and drill-in inspector.

## Testing Gates

Rust tests:

- Dashboard refuses to build or mount without auth.
- Bearer auth denies missing and invalid credentials.
- Bearer auth allows valid credentials.
- Dashboard routes mount under a configurable path.
- SQLite migrations run from empty database.
- SQLite pruning enforces age and count retention.
- In-memory storage works for tests.
- Metadata-only capture does not store payloads.
- Redaction removes configured headers and fields when payload capture is enabled.
- SSE stream emits live timeline records.
- Dashboard integration composes with Nidus/Axum routers.

Frontend/build tests:

- Dashboard assets build deterministically.
- Embedded assets are served under the dashboard path.
- UI can load from a non-root dashboard path.

Visual verification during implementation:

- Browser screenshot checks across desktop and mobile.
- Check that text does not overlap or overflow in navigation, timeline rows, inspector panels, and settings.
- Check that the page is not visually dominated by one hue family beyond the approved Nidus palette.

Security checks:

- No dashboard API or asset route bypasses auth.
- Unsafe auth-disabled mode is explicit and easy to search for.
- Payload capture remains off by default.
- Default redaction covers common secret headers and fields.

## Open Implementation Details

These details should be resolved in the implementation plan:

- Exact facade integration name: `with_dashboard(...)` vs an extension trait.
- Whether dashboard route/graph metadata is pushed from existing macros, collected from OpenAPI metadata, or both.
- Exact async write strategy for SQLite capture.
- Whether the frontend should be plain TypeScript, a small Vite app, or generated static assets built by a script.
- Exact crate feature names for SQLite, embedded assets, and facade integration.

## Approval

The design direction was approved for a first implementation plan:

- First option: embedded dashboard crate.
- Name: Nidus Dashboard.
- Optional crate, disabled by default unless installed and mounted.
- Embedded admin surface at `/nidus/dashboard`.
- Built-in zero-trust dashboard auth, extensible with user middleware/guards.
- SQLite default storage, user-configurable.
- Metadata-first capture by default, opt-in payload capture with redaction.
- Live app introspection only in v1.
- UI must strongly use `$impeccable` and carry the website style and palette.

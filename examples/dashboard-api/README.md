# Nidus Dashboard API Example

This example mounts Nidus Dashboard at `/nidus/dashboard` with bearer auth,
SQLite storage, metadata-only capture, route snapshots, event capture, job
capture, dashboard JSON APIs, an SSE stream, and normal app routes.

Run it:

```bash
export NIDUS_DASHBOARD_TOKEN=dev-dashboard-token
export NIDUS_DASHBOARD_DATABASE_URL='sqlite://./target/nidus-dashboard-example.sqlite?mode=rwc'
cargo run -p nidus-example-dashboard-api
```

Live checks:

```bash
curl http://127.0.0.1:4310/health
curl http://127.0.0.1:4310/hello/nidus

curl -i http://127.0.0.1:4310/nidus/dashboard/
curl -i -H 'Authorization: Bearer wrong' http://127.0.0.1:4310/nidus/dashboard/
curl -i -H 'Authorization: Bearer dev-dashboard-token' http://127.0.0.1:4310/nidus/dashboard/

curl -i http://127.0.0.1:4310/nidus/dashboard/api/overview
curl -H 'Authorization: Bearer dev-dashboard-token' http://127.0.0.1:4310/nidus/dashboard/api/overview
curl -H 'Authorization: Bearer dev-dashboard-token' http://127.0.0.1:4310/nidus/dashboard/api/routes

curl -X POST http://127.0.0.1:4310/events/user-created
curl -X POST http://127.0.0.1:4310/jobs/daily-digest
curl -H 'Authorization: Bearer dev-dashboard-token' http://127.0.0.1:4310/nidus/dashboard/api/timeline

curl -N -H 'Authorization: Bearer dev-dashboard-token' http://127.0.0.1:4310/nidus/dashboard/stream
```

Expected security behavior:

- missing dashboard bearer token returns `401`
- invalid dashboard bearer token returns `401`
- valid dashboard bearer token returns the dashboard shell and APIs
- application routes remain normal app routes
- payload capture is off by default

# Release 1.0.5

Nidus 1.0.5 ships the embedded dashboard runtime cockpit and aligns the public
release surface around the new opt-in `nidus-dashboard` crate.

## Runtime Dashboard

- Added the optional `nidus-dashboard` crate and facade `dashboard` feature for
  an embedded, protected runtime cockpit.
- The dashboard surfaces Home, Atlas, Routes, Timeline, Adapters, and Settings.
  Events and Jobs are consolidated into Timeline filters while their APIs and
  data collection remain available.
- The Home cockpit reports only data exposed by the dashboard backend: service,
  connection, auth, capture, storage, application shape, timeline health,
  recorded operation durations, and the latest timeline signals.
- Dashboard settings render backend modes as human-readable values, and active
  navigation or segmented controls expose accessibility state through ARIA.
- The embedded asset routes now include the Nidus logo and favicon assets under
  dashboard auth.

## Documentation And Examples

- `docs/dashboard.md` documents opt-in setup, builder configuration, auth,
  capture, storage, retention, UI sections, Timeline filters, API routes, and
  local bearer or auth-disabled demo flows.
- `examples/dashboard-api` demonstrates bearer auth, trusted local auth-disabled
  demos, SQLite storage, metadata-only capture, graph APIs, route snapshots,
  event/job capture, and SSE checks.
- The README, examples guide, installation guide, API reference, release notes,
  and website catalog now point at the 1.0.5 dashboard release surface.

## Release Boundary

After publishing, verify the public package and documentation state:

```bash
bash scripts/verify-published-release.sh 1.0.5
```

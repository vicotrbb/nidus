# API Reference

The release website links to generated Rust API references on docs.rs once the
crates are published. During local launch verification, build the same
reference set with:

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

After publishing, verify the docs.rs pages with:

```bash
bash scripts/verify-published-release.sh 1.0.8
```

| Crate | Surface | Reference |
| --- | --- | --- |
| `nidus-rs` | Facade crate and prelude | https://docs.rs/nidus-rs/1.0.8/nidus/ |
| `nidus-core` | Modules, DI, lifecycle, and app bootstrap | https://docs.rs/nidus-core/1.0.8/nidus_core/ |
| `nidus-http` | Controllers, routing, middleware, health, metrics, logging, OTel helpers | https://docs.rs/nidus-http/1.0.8/nidus_http/ |
| `nidus-macros` | Controller, route, module, provider, guard, pipe, and entrypoint macros | https://docs.rs/nidus-macros/1.0.8/nidus_macros/ |
| `nidus-config` | Typed configuration values and loaders | https://docs.rs/nidus-config/1.0.8/nidus_config/ |
| `nidus-openapi` | OpenAPI route metadata and document generation | https://docs.rs/nidus-openapi/1.0.8/nidus_openapi/ |
| `nidus-validation` | Validation pipes and JSON extractors backed by garde | https://docs.rs/nidus-validation/1.0.8/nidus_validation/ |
| `nidus-auth` | Guard traits, combinators, and Tower layers | https://docs.rs/nidus-auth/1.0.8/nidus_auth/ |
| `nidus-events` | Event bus and observed event dispatch | https://docs.rs/nidus-events/1.0.8/nidus_events/ |
| `nidus-jobs` | Job queues and observed job runners | https://docs.rs/nidus-jobs/1.0.8/nidus_jobs/ |
| `nidus-dashboard` | Optional embedded runtime cockpit, dashboard APIs, capture, auth, and storage | https://docs.rs/nidus-dashboard/1.0.8/nidus_dashboard/ |
| `nidus-testing` | TestApp request harness and provider overrides | https://docs.rs/nidus-testing/1.0.8/nidus_testing/ |
| `nidus-sqlx` | Official SQLx adapter | https://docs.rs/nidus-sqlx/1.0.8/nidus_sqlx/ |
| `nidus-cache` | Official Moka cache adapter | https://docs.rs/nidus-cache/1.0.8/nidus_cache/ |
| `cargo-nidus` | CLI generator and source inspector | https://docs.rs/cargo-nidus/1.0.8/ |

The facade crate keeps core Nidus ergonomic, while SQLx and cache integrations
remain separate installable crates.

# Deployment

Nidus applications are normal Rust binaries.

Recommended production defaults:

- build with `--release`
- configure addresses, logging, and secrets through typed config
- use `tracing` subscribers appropriate for the deployment platform
- place reverse proxy, TLS, compression, and rate limiting where they best fit the system

Nidus should not impose a hosting platform.

## Build

Build release binaries with Cargo:

```bash
cargo build --release
```

For workspace examples or generated applications, run the normal validation
commands before packaging:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

## Runtime Configuration

Keep runtime configuration outside the binary. Load defaults from explicit JSON
or pair sources, overlay environment variables with a prefix, deserialize into a
typed struct, and fail startup if required values are missing.

```rust
let config = Config::from_json_file("config/defaults.json")?
    .merge(Config::from_env_prefix("APP"));
let settings = config.deserialize::<AppConfig>()?;
```

Secrets should come from the deployment platform's secret mechanism and enter
the app through typed config, not hard-coded constants.

## HTTP Boundary

Nidus builds on Axum and Tower, so standard Rust deployment patterns apply:

- Bind to an explicit address such as `0.0.0.0:3000` in containers.
- Terminate TLS at the load balancer, reverse proxy, or app depending on your platform.
- Use Tower middleware or upstream infrastructure for compression, CORS, timeout, and rate limiting.
- Emit structured tracing events through a subscriber configured by the app.

## Health And Shutdown

Expose a simple health route for the platform's readiness checks. Keep startup
validation strict so a bad config, missing provider, invalid module graph, or
failed lifecycle hook prevents serving traffic.

When lifecycle hooks manage external resources, register them with clear startup
and shutdown behavior so tests and production shutdown paths exercise the same
logic.

# Deployment

Nidus applications are normal Rust binaries.

Recommended production defaults:

- build with `--release`
- configure addresses, logging, and secrets through typed config
- use `tracing` subscribers appropriate for the deployment platform
- place reverse proxy, TLS, compression, and rate limiting where they best fit the system

Nidus should not impose a hosting platform.


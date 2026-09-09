FROM rust:1.96.0-bookworm@sha256:5e2214abe154fe26e39f64488952e5c991eeed1d6d6da7cc8381ae83927f0cfc
RUN apt-get update && apt-get install -y --no-install-recommends clang cmake pkg-config libssl-dev libcurl4-openssl-dev python3 ruby jq ripgrep nodejs npm docker.io lsof && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy llvm-tools-preview
COPY --from=docker:29.4.0-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=node:24-bookworm-slim /usr/local/bin/node /usr/local/bin/node
WORKDIR /work

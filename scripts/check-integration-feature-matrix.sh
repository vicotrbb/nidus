#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-Dwarnings"

check() {
  echo "[feature-matrix] cargo check $*"
  cargo check "$@"
}

feature_crates=(
  nidus-integrations
  nidus-redis
  nidus-kafka
  nidus-nats
  nidus-rabbitmq
  nidus-sqs
  nidus-jobs-sqlx
  nidus-opentelemetry
  nidus-sentry
)

for crate in "${feature_crates[@]}"; do
  check -p "${crate}"
  check -p "${crate}" --no-default-features
  check -p "${crate}" --all-features
done

for feature in sqlite postgres mysql cockroach; do
  check -p nidus-sqlx --no-default-features --features "${feature}"
  check -p nidus-jobs-sqlx --no-default-features --features "${feature}"
done

check -p nidus-jobs

echo "[feature-matrix] all isolated, default, and all-feature checks passed"

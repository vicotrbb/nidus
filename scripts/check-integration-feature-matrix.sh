#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-Dwarnings"

check() {
  echo "[feature-matrix] cargo check --locked $*"
  cargo check --locked "$@"
}

assert_dependency_absent() {
  local dependency="$1"
  shift
  echo "[feature-matrix] assert ${dependency} is absent: cargo tree $*"
  if cargo tree --locked --prefix none "$@" |
    awk -v needle="${dependency} v" '
      index($0, needle) == 1 { found = 1 }
      END { exit(found ? 0 : 1) }
    '
  then
    echo "[feature-matrix] unexpected dependency: ${dependency}" >&2
    return 1
  fi
}

assert_dependency_present() {
  local dependency="$1"
  shift
  echo "[feature-matrix] assert ${dependency} is present: cargo tree $*"
  if ! cargo tree --locked --prefix none "$@" |
    awk -v needle="${dependency} v" '
      index($0, needle) == 1 { found = 1 }
      END { exit(found ? 0 : 1) }
    '
  then
    echo "[feature-matrix] missing dependency: ${dependency}" >&2
    return 1
  fi
}

feature_crates=(
  nidus-rs
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

check -p nidus-rs --no-default-features --features http
check -p nidus-rs --no-default-features --features observability
assert_dependency_absent tower-http -p nidus-rs --no-default-features
assert_dependency_present tower-http -p nidus-rs --no-default-features --features observability

for feature in sqlite postgres mysql cockroach; do
  check -p nidus-sqlx --no-default-features --features "${feature}"
  check -p nidus-jobs-sqlx --no-default-features --features "${feature}"
  if [[ "${feature}" != "sqlite" ]]; then
    assert_dependency_absent sqlx-sqlite -p nidus-sqlx --no-default-features --features "${feature}"
    assert_dependency_absent sqlx-sqlite -p nidus-jobs-sqlx --no-default-features --features "${feature}"
  fi
done

assert_dependency_present sqlx-sqlite -p nidus-sqlx --no-default-features --features sqlite
assert_dependency_present sqlx-sqlite -p nidus-jobs-sqlx --no-default-features --features sqlite
check -p nidus-dashboard --no-default-features --features sqlite
assert_dependency_present sqlx-sqlite -p nidus-dashboard --no-default-features --features sqlite

check -p nidus-jobs

echo "[feature-matrix] all isolated, default, and all-feature checks passed"

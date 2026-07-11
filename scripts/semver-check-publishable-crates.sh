#!/usr/bin/env bash
set -euo pipefail

temp_files=()
cleanup() {
  rm -f "${temp_files[@]}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

crates=(
  nidus-core
  nidus-macros
  nidus-config
  nidus-auth
  nidus-events
  nidus-jobs
  nidus-validation
  nidus-http
  nidus-testing
  nidus-openapi
  nidus-observability
  nidus-dashboard
  nidus-integrations
  nidus-opentelemetry
  nidus-sentry
  nidus-jobs-sqlx
  nidus-redis
  nidus-kafka
  nidus-nats
  nidus-rabbitmq
  nidus-sqs
  nidus-rs
  nidus-cache
  nidus-sqlx
  cargo-nidus
)

for crate in "${crates[@]}"; do
  echo "checking ${crate}"
  output="$(mktemp)"
  temp_files+=("${output}")
  set +e
  cargo semver-checks check-release --package "${crate}" "$@" 2>&1 | tee "${output}"
  status=${PIPESTATUS[0]}
  set -e

  if [ "${status}" -eq 0 ]; then
    rm -f "${output}"
    continue
  fi

  if grep -q "no crates with library targets selected" "${output}"; then
    echo "skipping ${crate}: no semver-checkable library target"
    rm -f "${output}"
    continue
  fi

  if grep -q "${crate} not found in registry" "${output}"; then
    echo "skipping ${crate}: no published baseline found"
    rm -f "${output}"
    continue
  fi

  rm -f "${output}"
  exit "${status}"
done

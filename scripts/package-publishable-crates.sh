#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}"

list_only=0
args=()
for arg in "$@"; do
  if [ "${arg}" = "--list-only" ]; then
    list_only=1
  else
    args+=("${arg}")
  fi
done

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

# Cargo stages interdependent archives in a temporary registry when selected
# together. Per-crate invocations instead resolve old published dependencies.
package_args=()
for crate in "${crates[@]}"; do
  package_args+=(-p "${crate}")
done
if [ "${list_only}" -eq 1 ]; then
  cargo package "${package_args[@]}" --list --allow-dirty "${args[@]}" >/dev/null
else
  cargo package "${package_args[@]}" "${args[@]}"
fi

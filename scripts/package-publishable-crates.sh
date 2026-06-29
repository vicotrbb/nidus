#!/usr/bin/env bash
set -euo pipefail

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
  nidus-rs
  nidus-cache
  nidus-sqlx
  cargo-nidus
)

for crate in "${crates[@]}"; do
  if [ "${list_only}" -eq 1 ]; then
    # CI uses the file-list preflight because full cargo package cannot walk
    # the 1.0.1 internal dependency chain until earlier crates are published.
    cargo package -p "${crate}" --list "${args[@]}" >/dev/null
  else
    cargo package -p "${crate}" "${args[@]}"
  fi
done

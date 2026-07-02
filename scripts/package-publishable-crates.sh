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
  nidus-dashboard
  nidus-rs
  nidus-cache
  nidus-sqlx
  cargo-nidus
)

for crate in "${crates[@]}"; do
  if [ "${list_only}" -eq 1 ]; then
    # CI uses the file-list preflight because full cargo package cannot walk
    # the current internal dependency chain until earlier crates are published.
    if [ "${#args[@]}" -gt 0 ]; then
      cargo package -p "${crate}" --list --allow-dirty "${args[@]}" >/dev/null
    else
      cargo package -p "${crate}" --list --allow-dirty >/dev/null
    fi
  else
    if [ "${#args[@]}" -gt 0 ]; then
      cargo package -p "${crate}" "${args[@]}"
    else
      cargo package -p "${crate}"
    fi
  fi
done

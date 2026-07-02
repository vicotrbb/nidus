#!/usr/bin/env bash
set -euo pipefail

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
  echo "checking ${crate}"
  output="$(mktemp)"
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

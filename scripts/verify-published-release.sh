#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-}"
CRATES_IO_USER_AGENT="${CRATES_IO_USER_AGENT:-nidus-release-verifier}"
DOCS_RS_MAX_ATTEMPTS="${DOCS_RS_MAX_ATTEMPTS:-30}"
DOCS_RS_SLEEP_SECONDS="${DOCS_RS_SLEEP_SECONDS:-20}"

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "$1" >&2
    exit 1
  }
}

require_command curl
require_command jq
require_command cargo

if [ -z "$VERSION" ]; then
  VERSION="$(
    cd "$ROOT"
    cargo metadata --no-deps --format-version 1 \
      | jq -r '.packages[] | select(.name == "nidus-rs") | .version'
  )"
fi

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

is_crate_published() {
  local crate="$1"
  local response
  response="$(
    curl \
      --fail \
      --silent \
      --show-error \
      --header "User-Agent: ${CRATES_IO_USER_AGENT}" \
      "https://crates.io/api/v1/crates/${crate}"
  )"

  jq -e --arg version "$VERSION" \
    '.versions[]? | select(.num == $version and (.yanked | not))' \
    >/dev/null <<<"$response"
}

verify_crates_io() {
  local crate
  for crate in "${crates[@]}"; do
    printf 'checking crates.io: %s %s\n' "$crate" "$VERSION"
    if ! is_crate_published "$crate"; then
      printf '%s %s is not published and unyanked on crates.io\n' "$crate" "$VERSION" >&2
      exit 1
    fi
  done
}

verify_docs_rs() {
  local index
  local crate
  local url
  local attempt
  local pending
  local next_pending
  local ready

  pending=("${!crates[@]}")
  for attempt in $(seq 1 "$DOCS_RS_MAX_ATTEMPTS"); do
    next_pending=()

    for index in "${pending[@]}"; do
      crate="${crates[$index]}"
      url="https://docs.rs/${crate}/${VERSION}/"
      printf 'checking docs.rs: %s (%s/%s)\n' "$url" "$attempt" "$DOCS_RS_MAX_ATTEMPTS"
      if curl -fsSIL --max-time 20 "$url" >/dev/null; then
        continue
      fi

      next_pending+=("$index")
    done

    if [ "${#next_pending[@]}" -eq 0 ]; then
      return 0
    fi

    if [ "$attempt" -eq "$DOCS_RS_MAX_ATTEMPTS" ]; then
      printf 'docs.rs did not serve these package pages after %s attempts:\n' "$DOCS_RS_MAX_ATTEMPTS" >&2
      for index in "${next_pending[@]}"; do
        crate="${crates[$index]}"
        printf '  https://docs.rs/%s/%s/\n' "$crate" "$VERSION" >&2
      done
      exit 1
    fi

    ready=$((${#pending[@]} - ${#next_pending[@]}))
    printf 'docs.rs ready this round: %s; pending: %s\n' "$ready" "${#next_pending[@]}"
    pending=("${next_pending[@]}")
    sleep "$DOCS_RS_SLEEP_SECONDS"
  done
}

printf 'verifying published Nidus release %s\n' "$VERSION"
verify_crates_io
verify_docs_rs

printf '\nverifying external examples against crates.io dependencies\n'
bash "$ROOT/scripts/verify-external-examples.sh"

printf '\npublished release verification passed for %s\n' "$VERSION"

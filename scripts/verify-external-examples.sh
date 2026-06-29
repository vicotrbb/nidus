#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ORIGINAL_SUPPORT_DIR="$ROOT/examples/external-support-desk"
ORIGINAL_COMMERCE_DIR="$ROOT/examples/external-commerce"
SUPPORT_DIR="$ORIGINAL_SUPPORT_DIR"
COMMERCE_DIR="$ORIGINAL_COMMERCE_DIR"
LOCAL_PATCH="${NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH:-0}"
SUPPORT_PORT=4301
COMMERCE_PORT=4302
SUPPORT_PID=""
COMMERCE_PID=""
TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nidus-external-examples.XXXXXX")"

cleanup() {
  if [ -n "$SUPPORT_PID" ] && kill -0 "$SUPPORT_PID" 2>/dev/null; then
    kill "$SUPPORT_PID" 2>/dev/null || true
    wait "$SUPPORT_PID" 2>/dev/null || true
  fi
  if [ -n "$COMMERCE_PID" ] && kill -0 "$COMMERCE_PID" 2>/dev/null; then
    kill "$COMMERCE_PID" 2>/dev/null || true
    wait "$COMMERCE_PID" 2>/dev/null || true
  fi
  rm -rf "$TEMP_DIR"
}
trap cleanup EXIT INT TERM

run() {
  printf '\n==> %s\n' "$*"
  "$@"
}

assert_no_external_path_dependencies() {
  if rg -n 'path *=.*nidus|/Users/victorbona/Daedalus/nidus' "$ORIGINAL_SUPPORT_DIR" "$ORIGINAL_COMMERCE_DIR"; then
    printf 'external examples must not contain local Nidus path dependencies\n' >&2
    exit 1
  fi
}

prepare_local_patch_examples() {
  if [ "$LOCAL_PATCH" != "1" ]; then
    return
  fi

  SUPPORT_DIR="$TEMP_DIR/external-support-desk"
  COMMERCE_DIR="$TEMP_DIR/external-commerce"
  cp -R "$ORIGINAL_SUPPORT_DIR" "$SUPPORT_DIR"
  cp -R "$ORIGINAL_COMMERCE_DIR" "$COMMERCE_DIR"

  cat >>"$SUPPORT_DIR/Cargo.toml" <<EOF_PATCH

[patch.crates-io]
nidus-rs = { path = "$ROOT/crates/nidus" }
nidus-testing = { path = "$ROOT/crates/nidus-testing" }
EOF_PATCH

  cat >>"$COMMERCE_DIR/Cargo.toml" <<EOF_PATCH

[patch.crates-io]
nidus-rs = { path = "$ROOT/crates/nidus" }
nidus-cache = { path = "$ROOT/crates/nidus-cache" }
nidus-sqlx = { path = "$ROOT/crates/nidus-sqlx" }
nidus-testing = { path = "$ROOT/crates/nidus-testing" }
EOF_PATCH

  printf '\n==> using temporary local [patch.crates-io] entries for unpublished Nidus crates\n'
}

wait_for_http() {
  url="$1"
  pid="$2"
  name="$3"
  deadline=$((SECONDS + 90))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if ! kill -0 "$pid" 2>/dev/null; then
      printf '%s server exited before becoming ready\n' "$name" >&2
      return 1
    fi
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  printf '%s server did not respond at %s\n' "$name" "$url" >&2
  return 1
}

request() {
  expected="$1"
  body_file="$2"
  shift 2
  status="$(curl -sS -o "$body_file" -w "%{http_code}" "$@")"
  if [ "$status" != "$expected" ]; then
    printf 'expected HTTP %s but got %s for: curl %s\n' "$expected" "$status" "$*" >&2
    printf 'response body:\n' >&2
    cat "$body_file" >&2
    printf '\n' >&2
    exit 1
  fi
}

assert_body_contains() {
  body_file="$1"
  pattern="$2"
  if ! grep -F "$pattern" "$body_file" >/dev/null; then
    printf 'expected response body to contain %s\n' "$pattern" >&2
    cat "$body_file" >&2
    printf '\n' >&2
    exit 1
  fi
}

run cd "$ROOT"
run cargo fmt --all --check
run cargo test -p cargo-nidus cargo_nidus_new_generates_compilable_nidus_project --all-features
assert_no_external_path_dependencies
prepare_local_patch_examples

run cargo fmt --manifest-path "$SUPPORT_DIR/Cargo.toml" --check
run cargo test --manifest-path "$SUPPORT_DIR/Cargo.toml"
run cargo fmt --manifest-path "$COMMERCE_DIR/Cargo.toml" --check
run cargo test --manifest-path "$COMMERCE_DIR/Cargo.toml"

printf '\n==> start support desk example\n'
(
  cd "$SUPPORT_DIR"
  NIDUS_ADDR="127.0.0.1:$SUPPORT_PORT" cargo run >"$TEMP_DIR/support.log" 2>&1
) &
SUPPORT_PID="$!"
wait_for_http "http://127.0.0.1:$SUPPORT_PORT/health/live" "$SUPPORT_PID" "support"

support_body="$TEMP_DIR/support-body.json"
request 201 "$support_body" -X POST "http://127.0.0.1:$SUPPORT_PORT/tickets" \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d78' \
  -d '{"subject":"Cannot deploy","description":"Pipeline is blocked","priority":"urgent"}'
assert_body_contains "$support_body" '"status":"open"'
assert_body_contains "$support_body" '"request_id":"018f4ad7-56ce-4f6a-a759-29f4438d8d78"'

request 200 "$support_body" -X POST "http://127.0.0.1:$SUPPORT_PORT/tickets/1/assign" \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -d '{"assignee":"Ada"}'
assert_body_contains "$support_body" '"status":"assigned"'

request 200 "$support_body" -X POST "http://127.0.0.1:$SUPPORT_PORT/tickets/1/comments" \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -d '{"author":"Grace","body":"Looking now"}'
assert_body_contains "$support_body" '"body":"Looking now"'

request 200 "$support_body" -X POST "http://127.0.0.1:$SUPPORT_PORT/tickets/1/close" \
  -H 'x-api-key: support-secret'
assert_body_contains "$support_body" '"status":"closed"'

request 401 "$support_body" "http://127.0.0.1:$SUPPORT_PORT/tickets"
request 400 "$support_body" -X POST "http://127.0.0.1:$SUPPORT_PORT/tickets" \
  -H 'content-type: application/json' \
  -H 'x-api-key: support-secret' \
  -d '{"subject":"","description":"missing subject","priority":"normal"}'
request 404 "$support_body" "http://127.0.0.1:$SUPPORT_PORT/tickets/404" \
  -H 'x-api-key: support-secret'

printf '\n==> start commerce example\n'
(
  cd "$COMMERCE_DIR"
  NIDUS_ADDR="127.0.0.1:$COMMERCE_PORT" cargo run >"$TEMP_DIR/commerce.log" 2>&1
) &
COMMERCE_PID="$!"
wait_for_http "http://127.0.0.1:$COMMERCE_PORT/health/live" "$COMMERCE_PID" "commerce"

commerce_body="$TEMP_DIR/commerce-body.json"
request 200 "$commerce_body" "http://127.0.0.1:$COMMERCE_PORT/health/ready"
request 200 "$commerce_body" "http://127.0.0.1:$COMMERCE_PORT/products"
assert_body_contains "$commerce_body" '"inventory":12'

request 201 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts" \
  -H 'content-type: application/json' \
  -d '{"id":"cart-1"}'
assert_body_contains "$commerce_body" '"id":"cart-1"'

request 200 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts/cart-1/items" \
  -H 'content-type: application/json' \
  -d '{"product_id":1,"quantity":2}'
assert_body_contains "$commerce_body" '"total_cents":5000'

request 200 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts/cart-1/checkout" \
  -H 'idempotency-key: checkout-1' \
  -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d88'
assert_body_contains "$commerce_body" '"total_cents":5000'
assert_body_contains "$commerce_body" '"request_id":"018f4ad7-56ce-4f6a-a759-29f4438d8d88"'

request 200 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts/cart-1/checkout" \
  -H 'idempotency-key: checkout-1'
assert_body_contains "$commerce_body" '"total_cents":5000'

request 200 "$commerce_body" "http://127.0.0.1:$COMMERCE_PORT/products"
assert_body_contains "$commerce_body" '"inventory":10'
request 200 "$commerce_body" "http://127.0.0.1:$COMMERCE_PORT/metrics"
assert_body_contains "$commerce_body" 'nidus_http_requests_total'

request 400 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts" \
  -H 'content-type: application/json' \
  -d '{"id":""}'
request 404 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts/missing/items" \
  -H 'content-type: application/json' \
  -d '{"product_id":1,"quantity":1}'
request 400 "$commerce_body" -X POST "http://127.0.0.1:$COMMERCE_PORT/carts/missing/checkout"

printf '\nexternal example verification passed\n'

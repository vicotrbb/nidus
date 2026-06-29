#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/nidus-live-examples.XXXXXX")"
SERVER_PID=""

cleanup() {
  if [ -n "${SERVER_PID}" ] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    pkill -TERM -P "${SERVER_PID}" 2>/dev/null || true
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  rm -rf "${TMP_DIR}" 2>/dev/null || true
}
trap cleanup EXIT

log() {
  printf '[live-examples] %s\n' "$*"
}

stop_server() {
  if [ -n "${SERVER_PID}" ] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    pkill -TERM -P "${SERVER_PID}" 2>/dev/null || true
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  SERVER_PID=""
}

kill_port() {
  local port="$1"
  local pids
  pids="$(lsof -ti "tcp:${port}" 2>/dev/null || true)"
  if [ -n "${pids}" ]; then
    log "clearing port ${port}"
    kill ${pids} 2>/dev/null || true
    sleep 0.5
  fi
}

url_port() {
  local url="$1"
  local host_port="${url#http://}"
  host_port="${host_port%%/*}"
  printf '%s\n' "${host_port##*:}"
}

start_server() {
  local name="$1"
  local url="$2"
  shift 2
  local log_file="${TMP_DIR}/${name}.log"
  local port
  port="$(url_port "${url}")"

  stop_server
  kill_port "${port}"
  log "starting ${name}"
  (
    cd "${ROOT}"
    "$@"
  ) >"${log_file}" 2>&1 &
  SERVER_PID="$!"

  for _ in $(seq 1 360); do
    if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
      cat "${log_file}" >&2
      log "${name} exited before becoming ready"
      return 1
    fi
    if curl -fsS "${url}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done

  cat "${log_file}" >&2
  log "${name} did not become ready at ${url}"
  return 1
}

body() {
  curl -fsS "$@"
}

expect_body_contains() {
  local expected="$1"
  shift
  local output
  output="$(body "$@")"
  if [[ "${output}" != *"${expected}"* ]]; then
    printf 'expected response to contain %q\nresponse:\n%s\n' "${expected}" "${output}" >&2
    return 1
  fi
}

expect_status() {
  local expected="$1"
  shift
  local body_file="${TMP_DIR}/response-body"
  local status
  status="$(curl -sS -o "${body_file}" -w '%{http_code}' "$@")"
  if [ "${status}" != "${expected}" ]; then
    printf 'expected HTTP %s, got %s for %s\nbody:\n' "${expected}" "${status}" "$*" >&2
    cat "${body_file}" >&2
    return 1
  fi
}

json_post() {
  local url="$1"
  local json="$2"
  shift 2
  body -X POST "${url}" -H 'content-type: application/json' "$@" -d "${json}"
}

run_http_examples() {
  start_server hello-world http://127.0.0.1:3000/ cargo run -p nidus-example-hello-world
  expect_body_contains 'hello from nidus' http://127.0.0.1:3000/
  stop_server

  start_server openapi http://127.0.0.1:3000/openapi.json cargo run -p nidus-example-openapi
  expect_body_contains 'Nidus Example API' http://127.0.0.1:3000/openapi.json
  expect_body_contains 'Nidus Example API Documentation' http://127.0.0.1:3000/docs
  stop_server

  start_server production-api http://127.0.0.1:3100/health/live env NIDUS_ADDR=127.0.0.1:3100 cargo run -p nidus-example-production-api
  expect_status 200 http://127.0.0.1:3100/health/live
  expect_status 200 http://127.0.0.1:3100/health/ready
  expect_body_contains 'nidus_http_requests_total' http://127.0.0.1:3100/metrics
  expect_status 408 http://127.0.0.1:3100/slow
  expect_status 404 http://127.0.0.1:3100/users/domain-error
  stop_server
}

run_realworld() {
  start_server realworld-api http://127.0.0.1:3200/health env NIDUS_BIND_ADDR=127.0.0.1:3200 cargo run -p nidus-example-realworld-api

  expect_body_contains '"status":"ok"' http://127.0.0.1:3200/health
  expect_body_contains '"status":"up"' http://127.0.0.1:3200/health/live
  expect_body_contains '"status":"up"' http://127.0.0.1:3200/health/ready
  expect_body_contains 'nidus_http_requests_total' http://127.0.0.1:3200/metrics
  expect_body_contains 'Nidus Real-World Team Tasks API' http://127.0.0.1:3200/openapi.json
  expect_body_contains 'Nidus Real-World Team Tasks API Documentation' http://127.0.0.1:3200/docs

  expect_status 422 -X POST http://127.0.0.1:3200/users -H 'content-type: application/json' -d '{}'
  expect_status 401 http://127.0.0.1:3200/projects/1

  json_post http://127.0.0.1:3200/users '{"email":"owner@nidus.dev","display_name":"Owner"}' | grep -q '"id":1'
  json_post http://127.0.0.1:3200/projects '{"owner_id":1,"name":"Launch API"}' -H 'x-api-key: dev-secret' | grep -q '"id":1'
  expect_body_contains '"name":"Launch API"' http://127.0.0.1:3200/projects/1 -H 'x-api-key: dev-secret'
  json_post http://127.0.0.1:3200/projects/1/tasks '{"title":"Write docs","description":"Document the real-world example"}' -H 'x-api-key: dev-secret' | grep -q '"id":1'
  expect_body_contains '"title":"Write docs"' http://127.0.0.1:3200/projects/1/tasks -H 'x-api-key: dev-secret'
  body -X PATCH http://127.0.0.1:3200/tasks/1/complete -H 'x-api-key: dev-secret' | grep -q '"completed":true'
  expect_body_contains '018f4ad7-56ce-4f6a-a759-29f4438d8d78' http://127.0.0.1:3200/context -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d78'
  json_post http://127.0.0.1:3200/ops/workflows/observed '{}' -H 'x-request-id: 018f4ad7-56ce-4f6a-a759-29f4438d8d78' | grep -q '"event_name":"task.completed"'
  stop_server
}

run_launchpad() {
  start_server launchpad-api http://127.0.0.1:3300/health env LAUNCHPAD_BIND_ADDR=127.0.0.1:3300 cargo run -p nidus-example-launchpad-api

  expect_body_contains '"status":"ok"' http://127.0.0.1:3300/health
  expect_body_contains 'Nidus Launchpad API' http://127.0.0.1:3300/openapi.json
  expect_body_contains 'nidus_http_requests_total' http://127.0.0.1:3300/metrics
  expect_status 401 -X POST http://127.0.0.1:3300/launches -H 'content-type: application/json' -d '{"name":"Nidus 1.0","owner_email":"owner@nidus.dev"}'
  expect_status 422 -X POST http://127.0.0.1:3300/launches -H 'content-type: application/json' -H 'x-api-key: launch-secret' -d '{"name":"","owner_email":"not-email"}'
  json_post http://127.0.0.1:3300/launches '{"name":"Nidus 1.0","owner_email":"owner@nidus.dev"}' -H 'x-api-key: launch-secret' | grep -q '"id":1'
  expect_body_contains '"status":"queued"' http://127.0.0.1:3300/launches/1 -H 'x-api-key: launch-secret'
  body -X PATCH http://127.0.0.1:3300/launches/1/ready -H 'x-api-key: launch-secret' | grep -q '"status":"ready"'
  body -X POST http://127.0.0.1:3300/ops/workflow -H 'x-api-key: launch-secret' | grep -q '"event_name":"launch.ready"'
  expect_status 413 -X POST http://127.0.0.1:3300/ops/webhook --data-binary 'abcdefghijklmnopqrstuvwxyz0123456789'
  stop_server
}

run_non_http_examples() {
  (cd "${ROOT}" && cargo run -p nidus-example-sqlx-app)
  (cd "${ROOT}" && cargo run -p nidus-example-cache-app)
  (cd "${ROOT}" && APP_DATABASE__URL=sqlite::memory: APP_CACHE__NAMESPACE=users cargo run -p nidus-example-integrations-production)
  (cd "${ROOT}" && cargo run -p nidus-example-background-jobs)
  (cd "${ROOT}" && cargo run -p nidus-example-modular-monolith)
}

run_generated_app() {
  local root="${TMP_DIR}/generated"
  mkdir -p "${root}"
  (cd "${ROOT}" && cargo run -p cargo-nidus -- nidus new live-generated --path "${root}" --nidus-path "${ROOT}/crates/nidus")
  start_server generated-app http://127.0.0.1:3400/ env NIDUS_ADDR=127.0.0.1:3400 cargo run --manifest-path "${root}/live-generated/Cargo.toml"
  expect_body_contains 'hello from nidus' http://127.0.0.1:3400/
  stop_server
}

run_http_examples
run_realworld
run_launchpad
run_non_http_examples
run_generated_app

log "live example verification complete"

#!/usr/bin/env bash
# Run inside an isolated Linux source snapshot with no host target directory.
set -u
cd "$(dirname "$0")/.."
export CARGO_NET_OFFLINE=true
proof_dir="$PWD/target/release-linux"
mkdir -p "$proof_dir"
: > "$proof_dir/outcomes.tsv"
failed=0
run_check() {
  local name="$1"
  shift
  local started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" > "$proof_dir/$name.log" 2>&1
  local status=$?
  printf '%s\t%s\t%s\n' "$name" "$status" "$((SECONDS-started))" >> "$proof_dir/outcomes.tsv"
  printf 'DONE %s status=%s seconds=%s\n' "$name" "$status" "$((SECONDS-started))"
  if [[ "$status" != 0 ]]; then failed=1; tail -n 15 "$proof_dir/$name.log"; fi
}
run_check format cargo fmt --all --check
run_check clippy cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
run_check tests cargo test --locked --workspace --all-features
run_check features bash scripts/check-integration-feature-matrix.sh
run_check docs env RUSTDOCFLAGS=-Dwarnings cargo doc --locked --workspace --all-features --no-deps
run_check live-examples bash scripts/verify-live-examples.sh
run_check external-examples env NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH=1 bash scripts/verify-external-examples.sh
run_check service-examples env NIDUS_VALIDATE_EXAMPLES=1 bash scripts/test-integration-services.sh
run_check website-domain npm --prefix website run verify:domain
run_check website-project npm --prefix website run verify:project
exit "$failed"

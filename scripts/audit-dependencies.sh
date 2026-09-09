#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

sentry_packages="$({
  awk '
    function emit() {
      if (name ~ /^sentry(-.*)?$/ && version != "") {
        print name " " version
      }
    }
    /^\[\[package\]\]$/ {
      emit()
      name = ""
      version = ""
      next
    }
    /^name = "/ {
      name = $0
      sub(/^name = "/, "", name)
      sub(/"$/, "", name)
      next
    }
    /^version = "/ {
      version = $0
      sub(/^version = "/, "", version)
      sub(/"$/, "", version)
    }
    END { emit() }
  ' Cargo.lock
} | sort -u)"

for required_package in sentry sentry-core sentry-tower sentry-tracing; do
  if ! grep -q "^${required_package} " <<<"${sentry_packages}"; then
    echo "required Sentry cohort package ${required_package} is missing from Cargo.lock" >&2
    exit 1
  fi
done

sentry_versions="$(awk '{print $2}' <<<"${sentry_packages}" | sort -u)"
if [[ "$(wc -l <<<"${sentry_versions}" | tr -d ' ')" -ne 1 ]]; then
  echo "Sentry crates must resolve as one exact release cohort" >&2
  printf '%s\n' "${sentry_packages}" >&2
  exit 1
fi

# cargo-audit treats a crates.io yank-status timeout as a non-fatal diagnostic.
# Keep yank enforcement in cargo-deny, where it is configured as a hard error,
# and disable cargo-audit's duplicate online yank query below.
cargo deny check advisories

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT
cargo metadata --locked --all-features --format-version 1 > "$metadata_file"
python3 scripts/check-security-graph.py "$metadata_file"

# SQLx 0.8 uses rsa only as a MySQL client-side RsaPublicKey for password
# encryption. It never owns or operates on an RSA private key, which is the
# secret exposed by RUSTSEC-2023-0071's non-constant-time private operations.
# Keep this one exception narrow and fail above if its direct reverse path
# changes. The rationale and required review conditions live in
# docs/security-notes.md.
cargo audit --deny warnings --no-yanked --ignore RUSTSEC-2023-0071

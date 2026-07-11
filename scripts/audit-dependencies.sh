#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

# cargo-audit treats a crates.io yank-status timeout as a non-fatal diagnostic.
# Keep yank enforcement in cargo-deny, where it is configured as a hard error,
# and disable cargo-audit's duplicate online yank query below.
cargo deny check advisories

rsa_path="$(cargo tree -p nidus-sqlx --all-features -i rsa)"
if ! grep -q '^└── sqlx-mysql ' <<<"${rsa_path}"; then
  echo "RUSTSEC-2023-0071 exception is no longer limited to sqlx-mysql" >&2
  printf '%s\n' "${rsa_path}" >&2
  exit 1
fi

# SQLx 0.8 uses rsa only as a MySQL client-side RsaPublicKey for password
# encryption. It never owns or operates on an RSA private key, which is the
# secret exposed by RUSTSEC-2023-0071's non-constant-time private operations.
# Keep this one exception narrow and fail above if its direct reverse path
# changes. The rationale and required review conditions live in
# docs/security-notes.md.
cargo audit --deny warnings --no-yanked --ignore RUSTSEC-2023-0071

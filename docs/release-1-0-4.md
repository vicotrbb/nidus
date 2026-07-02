# Release 1.0.4

Nidus 1.0.4 is a security hardening and release hygiene patch for the public
crate set.

## Security Hardening

- Updated `anyhow` from 1.0.102 to 1.0.103 to clear RUSTSEC-2026-0190.
- Pinned GitHub Actions workflow dependencies to full commit SHAs while keeping
  comments for the corresponding human-readable action tags.
- Expanded `SECURITY.md` with a direct private vulnerability reporting URL,
  email fallback, supported versions, response timeline, disclosure process, and
  scope.
- Added cargo-fuzz integration under `fuzz/` for route path normalization,
  OpenAPI metadata parsing, and config environment parsing surfaces.

## Release Boundary

The OpenSSF Best Practices badge alert is intentionally not addressed in this
patch. The repository-age maintained alert may remain until GitHub and OpenSSF
Scorecard can evaluate at least 90 days of history.

After publishing, verify the public package and documentation state:

```bash
bash scripts/verify-published-release.sh 1.0.4
```

## Summary

Describe the change and why it belongs in Nidus.

## Verification

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo doc --workspace --all-features --no-deps`
- [ ] `cargo deny check`
- [ ] `npm run verify` from `website/` when docs or site output changed

## Public API

- [ ] Public API changes have rustdoc.
- [ ] Docs, examples, or generated templates are updated when behavior changed.
- [ ] Macro diagnostics or CLI output changes have focused tests.

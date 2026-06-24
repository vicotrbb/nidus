# Contributing

Nidus is a Rust framework built around explicit APIs, strong typing, Axum/Tower
composition, and contributor-friendly internals. Contributions should preserve
those constraints even when adding NestJS-like ergonomics.

## Development Setup

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

Run focused tests while iterating, then run the full gate before opening a
pull request.

## Change Guidelines

- Keep changes small and reviewable.
- Prefer existing Rust ecosystem crates over custom infrastructure.
- Add tests for behavior changes and compile-fail tests for macro diagnostics.
- Update docs when public APIs, CLI output, or examples change.
- Keep generated code explicit enough to inspect and debug.
- Avoid hidden global state and runtime reflection.

## Public API Expectations

Public types and public functions should have rustdoc. Error messages should
name the invalid input and the expected shape. Generated projects should keep
working with `cargo check` and `cargo run`.

## Benchmarks

When changing routing, dependency resolution, middleware, or request handling,
run the relevant Criterion benchmark:

```bash
cargo bench --workspace --all-features
```

Document meaningful performance changes in `docs/performance.md`.

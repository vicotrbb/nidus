# Release 1.0.2

Nidus 1.0.2 is the public website, documentation, and launch-surface release
for the framework surface in this repository.

## Highlights

- Standalone modular Rust application structure with modules, controllers, providers, guards, validation, config, OpenAPI, events, jobs, testing, and production HTTP defaults.
- Lean `nidus-rs` facade package, imported as `nidus` in Rust code, with SQLx and cache integrations delivered as separately installable official adapters.
- `cargo-nidus` project generation plus source inspection commands for routes, module graphs, macro expansion, checks, and OpenAPI.
- Custom-domain website output for `rustnidus.com`, including root-base asset paths, generated `CNAME`, link checks, docs search, and a generated 404 page.
- Expanded public docs for installation, CLI, concepts, runtime surfaces, production boundaries, official adapters, examples, API reference, and release proof.

## Proof Boundary

Local verification can prove package dry-runs, tests, docs, website output, link
checks, visual rendering, and runtime examples. Actual crates.io publishing,
docs.rs rendering, GitHub Pages deployment settings, and DNS state require
credentials or external repository settings, so those steps must be reported
separately when blocked.

After publishing the crates, verify the public package and documentation state:

```bash
bash scripts/verify-published-release.sh 1.0.2
```

That command checks every publishable crate on crates.io, waits for each docs.rs
package page, and then runs the external examples against their checked-in
crates.io dependencies.

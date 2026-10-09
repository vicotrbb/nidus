# Compatible simplification

The 1.2.0 minor release retains `nidus_core::Provider`,
`nidus_validation::Pipe<Input>`, their facade prelude re-exports,
the `nidus_http::response` module, and concrete middleware service types.
Existing applications require no migration for this cleanup.

HTTP, OpenAPI, and CLI route string operations share an internal module in
`nidus-core`. OpenAPI conversion preserves whitespace and separators; HTTP and
CLI normalization trims outer whitespace and ensures a leading slash.
`nidus_http::error::RoutePathError` retains its original public definition;
HTTP and OpenAPI convert shared-helper errors at that boundary.
The CLI and OpenAPI crates explicitly depend on the existing core crate so
published packages carry the helper implementation without cross-crate source
file imports.

SQLite and Postgres use one telemetry recording helper. `CacheConfig` derives
`Default` with the same values. The custom middleware implementations remain
in place to preserve their public service and future types in a minor release.

# Simplification migration

The API cleanup removes `nidus_core::Provider`, `nidus_validation::Pipe<Input>`,
their facade prelude re-exports, and the empty `nidus_http::response` module.
These removals require a breaking release. The workspace version is unchanged;
release preparation must choose an appropriate breaking release version before publishing.

Replace `T: Provider` bounds with `T: Send + Sync + 'static`. The old marker
already had a blanket implementation for every type satisfying those bounds.

Replace custom `Pipe<Input>` implementations with inherent methods or ordinary
functions. Invoke transformations explicitly, then call
`ValidationPipe::transform` or use `ValidatedJson<T>` for JSON validation.
The `#[pipe(Type)]` attribute continues to describe route metadata.

Import response types from `nidus_http::{Response, IntoResponse}` or Axum.
The removed response module contained no items.

Security-header and request-context layer names remain available. Their service
names now alias Tower's `MapResponse` and `MapRequest` types. The stateful
request-scope implementation remains unchanged to preserve its generic request
body support without adding allocation or type erasure.

HTTP, OpenAPI, and CLI route string operations share an internal module in
`nidus-core`. OpenAPI conversion preserves whitespace and separators; HTTP and
CLI normalization trims outer whitespace and ensures a leading slash.
`nidus_http::error::RoutePathError` remains available as a re-export.
The CLI and OpenAPI crates explicitly depend on the existing core crate so
published packages carry the helper implementation without cross-crate source
file imports. No third-party dependency was added or removed.

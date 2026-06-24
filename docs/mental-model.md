# Mental Model

Nidus applications are explicit Rust programs organized around modules.

- Modules describe ownership boundaries.
- Providers are typed Rust values.
- Controllers compose Axum routes.
- Guards decide whether a request may continue.
- Pipes transform or validate request data.
- Interceptors and middleware use Tower layers where possible.

Nidus should feel ergonomic at the call site, but the generated and runtime
behavior must remain inspectable.

## What Happens At Build Time

Procedural macros validate syntax and generate small, explicit Rust fragments:
module definitions, provider registration helpers, controller prefixes, route
metadata, and entrypoint runtime setup. Invalid macro usage should fail during
compilation with an actionable error.

## What Happens At Startup

Applications bootstrap from a root module and an explicit list of imported
module definitions when imports are needed. The module graph validates duplicate
modules, missing imports, invalid exports, ambiguous imported providers, and
circular imports before the app is considered bootstrapped.

The container owns typed provider registrations. It resolves providers by Rust
type, not runtime reflection or string lookup. Singleton providers are reused,
transient providers are recreated, and request providers require an explicit
request scope.

## What Happens Per Request

HTTP handling remains Axum and Tower. Nidus route definitions compose into Axum
routers, middleware is Tower-based, and request-scoped providers are opt-in. The
default request path should not rebuild or traverse the dependency graph.

## How To Think About Magic

Use macros for concise declarations when they produce obvious generated code.
Use plain Rust factories, functions, traits, and Axum types whenever they make
the behavior clearer. Nidus should organize code, not replace Rust's normal
debugging and tooling model.

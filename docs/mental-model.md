# Mental Model

Nidus applications are explicit Rust programs organized around modules.

- Modules describe ownership boundaries.
- Providers are typed Rust values.
- Controllers compose Axum routes.
- Guards decide whether a request may continue.
- Pipes transform or validate request data.
- Interceptors and middleware use Tower layers where possible.

Nidus should feel ergonomic at the call site, but the generated and runtime behavior must remain inspectable.


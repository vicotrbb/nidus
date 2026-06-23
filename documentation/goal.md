# /goal Build `nidus`: A modular Rust application framework inspired by NestJS

You are implementing **nidus**, a production-grade, open-source Rust application framework.

`nidus` is a modular backend framework inspired by the developer experience of NestJS, but designed according to Rust principles: explicitness, compile-time safety, performance, low runtime overhead, strong typing, and excellent maintainability.

Website/domain: `nidus.dev`

The goal is to build a complete, high-quality Rust framework that provides:

- Modular application architecture
- Compile-time dependency injection
- Controllers and route macros
- Providers/services
- Guards
- Pipes/validation
- Interceptors/middleware
- Typed configuration
- Error handling
- OpenAPI generation
- Testing utilities
- CLI tooling
- Excellent examples and documentation
- Benchmarks and performance validation

Nidus must feel ergonomic like NestJS, but must never copy NestJS runtime magic. It must embrace Rust.

---

## Core philosophy

Nidus should provide:

```txt
NestJS-like ergonomics
Rust-native correctness
Axum/Tower/Tokio performance
Compile-time dependency validation
Explicit generated code
Clean project organization
Production-ready defaults
```

The central principle:

```txt
Nidus should feel magical to use,
but never be magical at runtime.
```

---

## Implementation loop

Work in an iterative loop until the framework is complete, tested, validated, optimized, documented, and production-ready.

Every iteration must follow these four steps:

### 1. Implement

Implement the smallest valuable slice of functionality with clean Rust code, good module boundaries, and minimal complexity.

### 2. Test

Add or update tests for everything implemented.

Required test types:

- Unit tests
- Integration tests
- Compile-fail tests for macros and invalid DI graphs
- Snapshot tests for generated macro output where useful
- Example app tests
- Documentation tests
- Regression tests for bugs found during validation

### 3. Validate

Validate correctness, ergonomics, generated code, public API design, project structure, feature completeness, and developer experience.

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
cargo bench
```

When applicable, also run:

```bash
cargo expand
cargo miri test
cargo audit
cargo deny check
```

### 4. Optimize / Improve

Improve performance, API ergonomics, compile times, documentation, error messages, module boundaries, and internal simplicity.

Then repeat the loop.

Never stop after a feature “works.” Continue improving until it is clean, tested, documented, benchmarked, and idiomatic.

---

## Master rules

Follow these rules at all times.

### Must do

- Write idiomatic Rust.
- Prefer compile-time safety over runtime checks.
- Prefer explicit code generation over hidden runtime behavior.
- Keep APIs ergonomic but predictable.
- Keep public APIs small, consistent, and documented.
- Design for real production use.
- Keep crates focused and cohesive.
- Use strong typing everywhere.
- Use useful compiler errors where possible.
- Make generated code inspectable.
- Add examples for every major feature.
- Add tests for every major behavior.
- Benchmark core runtime paths.
- Treat documentation as part of the product.
- Keep the framework modular and extensible.
- Preserve compatibility with standard Rust ecosystem tools.
- Build on top of proven libraries instead of reinventing them.

### Must not do

- Do not build a custom HTTP server.
- Do not replace Axum, Tower, or Tokio.
- Do not use runtime reflection.
- Do not use string-based dependency tokens by default.
- Do not create global mutable registries.
- Do not hide important behavior inside runtime magic.
- Do not overuse procedural macros when plain Rust is better.
- Do not generate unreadable code.
- Do not create huge files.
- Do not place unrelated concerns in the same module.
- Do not introduce circular dependencies between crates.
- Do not accept poor compiler errors from macros.
- Do not add features without tests.
- Do not add dependencies casually.
- Do not use `unsafe` unless absolutely necessary and justified.
- Do not optimize prematurely before measuring.
- Do not break Rust ecosystem expectations.
- Do not make Nidus feel like TypeScript translated to Rust.

---

## Technical foundation

Use the Rust ecosystem directly:

```txt
axum      HTTP routing and handlers
tower     middleware and service abstraction
tokio     async runtime
serde     serialization
thiserror error definitions
tracing   logging and observability
utoipa    OpenAPI generation
sqlx      first-class database example/integration
clap      CLI
trybuild  compile-fail macro tests
criterion benchmarks
insta     snapshots where useful
```

Nidus should compose these tools instead of hiding them completely.

---

## Workspace structure

Create a Cargo workspace with this structure:

```txt
nidus/
├── Cargo.toml
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── CHANGELOG.md
├── rustfmt.toml
├── clippy.toml
├── deny.toml
├── crates/
│   ├── nidus/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── prelude.rs
│   ├── nidus-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app/
│   │       ├── container/
│   │       ├── module/
│   │       ├── provider/
│   │       ├── lifecycle/
│   │       └── error/
│   ├── nidus-macros/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── module.rs
│   │       ├── controller.rs
│   │       ├── injectable.rs
│   │       ├── routes.rs
│   │       ├── guard.rs
│   │       ├── pipe.rs
│   │       ├── utils.rs
│   │       └── diagnostics.rs
│   ├── nidus-http/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── router.rs
│   │       ├── controller.rs
│   │       ├── request.rs
│   │       ├── response.rs
│   │       ├── middleware.rs
│   │       └── error.rs
│   ├── nidus-config/
│   ├── nidus-openapi/
│   ├── nidus-validation/
│   ├── nidus-auth/
│   ├── nidus-events/
│   ├── nidus-jobs/
│   ├── nidus-testing/
│   └── cargo-nidus/
├── examples/
│   ├── hello-world/
│   ├── rest-api/
│   ├── auth-api/
│   ├── sqlx-postgres/
│   ├── openapi/
│   ├── background-jobs/
│   └── modular-monolith/
├── tests/
│   ├── integration/
│   ├── compile-fail/
│   └── snapshots/
├── benches/
│   ├── routing.rs
│   ├── dependency_resolution.rs
│   └── request_lifecycle.rs
└── docs/
    ├── getting-started.md
    ├── modules.md
    ├── dependency-injection.md
    ├── controllers.md
    ├── providers.md
    ├── guards.md
    ├── pipes.md
    ├── interceptors.md
    ├── config.md
    ├── testing.md
    ├── openapi.md
    ├── performance.md
    └── architecture.md
```

No file should become a dumping ground. Split code by responsibility.

Recommended limits:

- Avoid files over 300 lines unless justified.
- Avoid modules with unrelated responsibilities.
- Avoid public APIs leaking internal implementation details.
- Keep macros internally organized by feature.
- Keep generated code simple and predictable.

---

## Public API target

The framework should eventually support this kind of user code:

```rust
use nidus::prelude::*;

#[module]
pub struct AppModule {
    imports: [ConfigModule, DatabaseModule, UsersModule],
}

#[module]
pub struct UsersModule {
    providers: [UsersService, UsersRepository],
    controllers: [UsersController],
}

#[injectable]
pub struct UsersRepository {
    db: Inject<PgPool>,
}

#[injectable]
pub struct UsersService {
    repo: Inject<UsersRepository>,
}

#[controller("/users")]
pub struct UsersController {
    users: Inject<UsersService>,
}

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_one(&self, id: Path<Uuid>) -> Result<Json<UserDto>, AppError> {
        let user = self.users.find_by_id(id.0).await?;
        Ok(Json(user))
    }

    #[post("/")]
    async fn create(&self, Json(input): Json<CreateUserDto>) -> Result<Json<UserDto>, AppError> {
        let user = self.users.create(input).await?;
        Ok(Json(user))
    }
}

#[nidus::main]
async fn main() -> Result<(), NidusError> {
    Nidus::bootstrap::<AppModule>()
        .listen("0.0.0.0:3000")
        .await
}
```

---

## Dependency injection design

DI must be Rust-native.

Use:

- Typed dependencies
- `Arc<T>` or framework wrapper types where needed
- Compile-time graph validation where possible
- Clear startup errors only where compile-time validation is impossible
- No string tokens by default
- No runtime reflection

Support:

```rust
Inject<T>
Optional<T>
Lazy<T>
Factory<T>
Scoped<T>
```

Provider lifetimes:

```txt
Singleton    created once at application boot
Transient    created when requested
Request      created per request, only when explicitly enabled
```

Default should be singleton.

Request-scoped dependencies must be opt-in because they can hurt performance.

---

## Module system

Modules are the core organizational unit.

Support:

```rust
#[module]
pub struct UsersModule {
    imports: [DatabaseModule],
    providers: [UsersService, UsersRepository],
    controllers: [UsersController],
    exports: [UsersService],
}
```

Rules:

- Imports must be explicit.
- Exports must be explicit.
- Circular imports must be detected.
- Missing providers must produce useful errors.
- Ambiguous providers must produce useful errors.
- Modules should compile into explicit registration code.

---

## HTTP system

Use Axum internally.

Support route macros:

```rust
#[controller("/users")]
pub struct UsersController {
    users: Inject<UsersService>,
}

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_one(&self, id: Path<Uuid>) -> Result<Json<UserDto>, AppError> {
        ...
    }

    #[post("/")]
    async fn create(&self, body: Json<CreateUserDto>) -> Result<Json<UserDto>, AppError> {
        ...
    }
}
```

Support:

- `GET`
- `POST`
- `PUT`
- `PATCH`
- `DELETE`
- route params
- query params
- JSON bodies
- typed responses
- error responses
- guards
- pipes
- middleware/interceptors
- OpenAPI metadata

---

## Guards

Guards should be composable and typed.

Example:

```rust
#[guard(AuthGuard)]
#[get("/me")]
async fn me(&self, user: CurrentUser) -> Result<Json<UserDto>, AppError> {
    ...
}
```

Guard trait:

```rust
pub trait Guard<S>: Send + Sync + 'static {
    async fn check(&self, ctx: GuardContext<S>) -> Result<(), GuardError>;
}
```

---

## Pipes and validation

Support request transformation and validation.

Example:

```rust
#[post("/")]
#[validate]
async fn create(&self, body: Json<CreateUserDto>) -> Result<Json<UserDto>, AppError> {
    ...
}
```

Validation should integrate with `validator` or a similarly appropriate crate, but must remain modular.

---

## Interceptors and middleware

Use Tower layers where possible.

Support:

- Logging
- Tracing
- Timeout
- Compression
- CORS
- Rate limiting
- Request IDs
- Metrics hooks

Do not invent a parallel middleware ecosystem unless necessary.

---

## Error handling

Create a clean framework error model.

Support:

```rust
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("user not found")]
    NotFound,

    #[error("database error")]
    Database(#[from] sqlx::Error),
}
```

Allow mapping errors to HTTP responses:

```rust
impl IntoResponse for AppError {
    ...
}
```

Nidus should provide good defaults but allow full customization.

---

## OpenAPI

Generate OpenAPI from controller metadata and DTOs.

Support:

```rust
#[derive(Serialize, Deserialize, ToSchema)]
pub struct UserDto {
    id: Uuid,
    email: String,
}

#[get("/:id")]
#[openapi(summary = "Find user by ID")]
async fn find_one(...) -> Result<Json<UserDto>, AppError> {
    ...
}
```

Expose:

```txt
/openapi.json
/docs
```

---

## CLI

Create `cargo-nidus`.

Commands:

```bash
cargo nidus new my-api
cargo nidus generate module users
cargo nidus generate controller users
cargo nidus generate service users
cargo nidus generate repository users
cargo nidus routes
cargo nidus graph
cargo nidus expand
cargo nidus check
cargo nidus openapi
```

The CLI must create clean, idiomatic project structures.

---

## Testing framework

Provide `nidus-testing`.

Example:

```rust
#[tokio::test]
async fn creates_user() {
    let app = TestApp::bootstrap::<AppModule>()
        .override_provider::<UsersRepository>(MockUsersRepository::new())
        .await;

    let response = app
        .post("/users")
        .json(&CreateUserDto { ... })
        .send()
        .await;

    response.assert_status(StatusCode::CREATED);
}
```

Testing support must include:

- Provider overrides
- Mock injection
- HTTP request helpers
- In-memory app boot
- Config overrides
- Test lifecycle hooks

---

## Observability

Use `tracing`.

Support:

- Request spans
- Route labels
- Error spans
- Module startup logs
- Dependency graph debug logs
- Optional metrics hooks

Do not force a specific metrics backend.

---

## Performance goals

Nidus should add minimal overhead over raw Axum.

Performance requirements:

- No per-request dependency graph resolution by default.
- No string lookup in hot paths.
- No reflection.
- No unnecessary boxing.
- No unnecessary cloning.
- No global locks in request path.
- Prefer static dispatch where practical.
- Use `Arc` intentionally.
- Benchmark against equivalent Axum apps.

Benchmark targets:

```txt
Raw Axum baseline
Nidus hello-world app
Nidus controller + service app
Nidus guarded route
Nidus validation route
```

Document overhead honestly.

---

## Documentation requirements

Every crate must have crate-level documentation.

Every public type must be documented.

The docs must include:

- Getting started
- Mental model
- Modules
- Dependency injection
- Controllers
- Providers
- Guards
- Pipes
- Interceptors
- Config
- OpenAPI
- Testing
- Performance
- Deployment
- Examples
- Migration from NestJS concepts

README must include:

- What Nidus is
- What Nidus is not
- Quickstart
- Example app
- Features
- Status
- Roadmap
- Contributing
- License

---

## Code quality standards

Use:

```bash
cargo fmt
cargo clippy
cargo test
cargo doc
cargo bench
```

No warnings allowed.

Use strict linting where reasonable.

Prefer:

- Small modules
- Clear names
- Explicit types
- Good error messages
- Low coupling
- High cohesion
- Trait-based extension points
- Feature flags for optional integrations

Avoid:

- Over-engineering
- Macro abuse
- Hidden global state
- Runtime type guessing
- Giant enums
- Giant files
- Circular crate dependencies
- Unnecessary abstractions

---

## Feature flags

Use feature flags carefully.

Possible features:

```toml
default = ["http", "config", "tracing"]

http = ["nidus-http"]
config = ["nidus-config"]
openapi = ["nidus-openapi"]
validation = ["nidus-validation"]
auth = ["nidus-auth"]
events = ["nidus-events"]
jobs = ["nidus-jobs"]
testing = ["nidus-testing"]
sqlx-postgres = ["sqlx/postgres"]
```

Optional integrations must not bloat the core framework.

---

## Release quality checklist

Before considering the goal complete, verify:

- Workspace builds cleanly.
- All tests pass.
- Compile-fail tests cover bad macro usage.
- Examples compile and run.
- Documentation builds.
- Benchmarks exist.
- Public API is coherent.
- CLI works.
- Generated projects compile.
- Generated modules are clean.
- Error messages are useful.
- Dependency graph validation works.
- Missing providers are detected.
- Circular dependencies are detected.
- OpenAPI output works.
- Test harness works.
- Performance overhead is measured.
- README is strong.
- Contributing guide exists.
- License files exist.
- Security policy exists.

---

## Final deliverable

The final repository must contain a working, documented, tested, benchmarked Rust framework called `nidus`.

It must be possible to run:

```bash
cargo nidus new hello-nidus
cd hello-nidus
cargo run
```

And get a working HTTP server.

It must be possible to define modules, services, controllers, and routes with ergonomic macros.

It must be possible to test an app with provider overrides.

It must be possible to generate OpenAPI documentation.

It must be possible to inspect routes and dependency graph using the CLI.

The codebase must be clean enough for open-source contributors to understand and extend.

Do not stop when the first version works. Continue the implement → test → validate → optimize/improve loop until the framework is production-quality.

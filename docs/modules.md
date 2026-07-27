# Modules

Modules group imports, providers, controllers, and exports.

Use `#[module]` when the module is only metadata:

```rust
use nidus::prelude::*;

#[module]
pub struct UsersModule {
    imports: (DatabaseModule,),
    providers: (UsersRepository, UsersService),
    controllers: (UsersController,),
    exports: (UsersService,),
}
```

The tuple syntax is intentional: module fields are compile-time metadata that
the macro lowers to an explicit `ModuleBuilder` definition. Use tuple groups for
multiple entries because Rust field type syntax does not allow comma-separated
`[A, B]` lists before the attribute macro runs. Single-entry bracket groups such
as `providers: [UsersService]` are also accepted.

```rust
use nidus_core::ModuleBuilder;

let users = ModuleBuilder::new("UsersModule")
    .import("DatabaseModule")
    .provider("UsersRepository")
    .provider("UsersService")
    .controller("UsersController")
    .export("UsersService")
    .build();
```

The module graph validates duplicate module names, duplicate local imports,
providers, controllers, and exports, provider/controller name conflicts, missing
imports, circular imports, invalid exports, local providers that conflict with
imported exports, and ambiguous imported providers before an application is
considered bootstrapped.

Typed imports produced by `#[module]` or `ModuleBuilder::import_typed` are
followed recursively from the root module. String-only imports created with
`ModuleBuilder::import` have no Rust factory to follow, so pass those explicit
definitions at bootstrap:

```rust
let app = Nidus::bootstrap_with_modules::<AppModule, _>([
    UsersModule::definition(),
])?;
```

When startup hooks are needed, validate the same explicit graph before running
the lifecycle runner:

```rust
let app = Nidus::bootstrap_with_modules_and_lifecycle::<AppModule, _>(
    [UsersModule::definition()],
    lifecycle,
)
.await?;
```

Provider bootstrap has two phases. Nidus first runs every synchronous provider
registrar, then runs async provider initializers when using a lifecycle-aware or
facade builder entrypoint. Imported modules run before their importers in both
phases, so an importer callback can resolve a resource installed by an imported
module. Async initializers remain sequential and the first error stops
bootstrap. `ModuleGraph::modules()` is still name-ordered for inspection; that
iterator is not provider execution order.

Lifecycle startup runs hooks in registration order. If a startup hook fails,
Nidus shuts down already-started hooks in reverse order before returning a
`LifecycleStartup` error that preserves the original failure and any rollback
shutdown failures.

Lifecycle startup, shutdown, and rollback emit `tracing` events with hook
indexes and hook counts. Applications can collect those events with any
`tracing` subscriber without coupling Nidus to a specific logging backend.
Shutdown attempts every hook in reverse registration order even when a hook
fails, then returns the first shutdown error. This keeps one adapter failure
from preventing unrelated resources from receiving their cleanup callback.

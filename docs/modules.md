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

Applications with imports bootstrap by passing the root module type plus the
explicit imported module definitions:

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

Lifecycle startup runs hooks in registration order. If a startup hook fails,
Nidus shuts down already-started hooks in reverse order before returning a
`LifecycleStartup` error that preserves the original failure and any rollback
shutdown failures.

Lifecycle startup, shutdown, and rollback emit `tracing` events with hook
indexes and hook counts. Applications can collect those events with any
`tracing` subscriber without coupling Nidus to a specific logging backend.

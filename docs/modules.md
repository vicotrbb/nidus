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

The tuple syntax is intentional: module fields are compile-time metadata that the macro lowers to an explicit `ModuleBuilder` definition.

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

The module graph validates duplicate module names, duplicate local imports, providers, controllers, and exports, missing imports, circular imports, invalid exports, and ambiguous imported providers before an application is considered bootstrapped.

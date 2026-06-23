# Modules

Modules group imports, providers, controllers, and exports.

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

The module graph validates missing imports and circular imports before an application is considered bootstrapped.


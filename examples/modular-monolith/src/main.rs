//! Macro-defined module graph example for a modular monolith shape.

use nidus::prelude::*;

#[allow(dead_code)]
struct DatabasePool;
#[allow(dead_code)]
struct UsersRepository;
#[allow(dead_code)]
struct UsersService;
#[allow(dead_code)]
struct UsersController;

#[module]
struct DatabaseModule {
    providers: (DatabasePool,),
    exports: (DatabasePool,),
}

#[module]
struct UsersModule {
    imports: (DatabaseModule,),
    providers: (UsersRepository, UsersService),
    controllers: (UsersController,),
    exports: (UsersService,),
}

fn main() {
    let graph =
        ModuleGraph::from_modules([DatabaseModule::definition(), UsersModule::definition()])
            .unwrap();

    assert!(graph.get("UsersModule").is_some());
    assert_eq!(
        graph.get("UsersModule").unwrap().providers(),
        ["UsersRepository", "UsersService"]
    );
}

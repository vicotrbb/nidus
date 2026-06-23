use nidus_core::{ModuleBuilder, ModuleGraph};

fn main() {
    let database = ModuleBuilder::new("DatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let users = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .provider("UsersService")
        .controller("UsersController")
        .build();

    let graph = ModuleGraph::from_modules([database, users]).unwrap();
    assert!(graph.get("UsersModule").is_some());
}

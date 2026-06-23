use nidus::prelude::*;

struct DatabaseModule;
struct UsersService;
struct UsersRepository;
struct UsersController;

#[module]
pub struct UsersModule {
    imports: (DatabaseModule,),
    providers: (UsersService, UsersRepository),
    controllers: (UsersController,),
    exports: (UsersService,),
}

fn main() {
    let definition = UsersModule::definition();
    assert_eq!(definition.name(), "UsersModule");
    assert_eq!(definition.imports(), ["DatabaseModule"]);
    assert_eq!(definition.providers(), ["UsersService", "UsersRepository"]);
    assert_eq!(definition.controllers(), ["UsersController"]);
    assert_eq!(definition.exports(), ["UsersService"]);
}

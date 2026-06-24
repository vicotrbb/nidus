use nidus::prelude::*;

#[module]
struct DatabaseModule;

#[injectable]
struct UsersService;

#[injectable]
struct UsersRepository;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/")]
    async fn index(&self) {}
}

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

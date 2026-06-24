use nidus::prelude::*;

#[module]
struct DatabaseModule;

#[injectable]
struct UsersService;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/")]
    async fn index(&self) {}
}

#[module]
pub struct UsersModule {
    imports: [DatabaseModule],
    providers: [UsersService],
    controllers: [UsersController],
    exports: [UsersService],
}

fn main() {
    let definition = UsersModule::definition();
    assert_eq!(definition.imports(), ["DatabaseModule"]);
    assert_eq!(definition.providers(), ["UsersService"]);
    assert_eq!(definition.controllers(), ["UsersController"]);
    assert_eq!(definition.exports(), ["UsersService"]);
}

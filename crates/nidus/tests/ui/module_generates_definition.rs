use nidus::prelude::*;

struct UsersService;
struct UsersController;

#[module(
    providers(UsersService),
    controllers(UsersController),
    exports(UsersService)
)]
pub struct UsersModule;

fn main() {
    let definition = UsersModule::definition();
    assert_eq!(definition.name(), "UsersModule");
    assert_eq!(definition.providers(), ["UsersService"]);
    assert_eq!(definition.controllers(), ["UsersController"]);
    assert_eq!(definition.exports(), ["UsersService"]);
}

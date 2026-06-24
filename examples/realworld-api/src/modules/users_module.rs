use nidus::prelude::module;

#[module]
pub struct UsersModule {
    providers: (crate::users::UsersRepository, crate::users::UsersService),
    controllers: [crate::users::UsersController],
    exports: [crate::users::UsersService],
}

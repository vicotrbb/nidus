use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[post("/")]
    #[openapi(summary = "Create user", request = "CreateUserDto")]
    async fn create(&self) {}
}

fn main() {}

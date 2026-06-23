use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[post("/")]
    #[pipe]
    async fn create(&self) {}
}

fn main() {}

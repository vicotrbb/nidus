use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[guard]
    async fn find_one(&self) {}
}

fn main() {}

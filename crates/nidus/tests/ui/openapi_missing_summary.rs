use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[openapi(description = "Find user by ID")]
    async fn find_one(&self) {}
}

fn main() {}

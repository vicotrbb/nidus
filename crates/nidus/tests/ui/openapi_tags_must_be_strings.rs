use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[openapi(summary = "Find user by ID", tags = ["users", 42])]
    async fn find_one(&self) {}
}

fn main() {}

use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[openapi(summary = "Find user")]
    #[openapi(summary = "Duplicate metadata")]
    async fn find_one(&self) {}
}

fn main() {}

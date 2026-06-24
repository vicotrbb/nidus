use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[openapi(summary = "Find user", status = 99)]
    async fn find_one(&self) {}
}

fn main() {}

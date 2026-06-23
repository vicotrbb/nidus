use nidus::prelude::*;

struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[post("/")]
    async fn find_one(&self) {}
}

fn main() {}

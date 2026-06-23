use nidus::prelude::*;

struct UsersController;

#[routes]
impl UsersController {
    #[get("/:")]
    async fn find_one(&self) {}
}

fn main() {}

use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_one(&self) {}

    #[post("/")]
    async fn create(&self) {}
}

fn main() {
    let routes = UsersController::routes();
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].method(), "GET");
    assert_eq!(routes[0].path(), "/:id");
    assert_eq!(routes[1].method(), "POST");
    assert_eq!(routes[1].path(), "/");
}

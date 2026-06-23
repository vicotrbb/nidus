use nidus::prelude::*;

#[controller("/users")]
struct UsersController;
struct AuthGuard;
struct ValidationPipe;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[guard(AuthGuard)]
    #[pipe(ValidationPipe)]
    #[openapi(summary = "Find user by ID")]
    async fn find_one(&self) {}

    #[post("/")]
    async fn create(&self) {}
}

fn main() {
    let routes = UsersController::routes();
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].method(), "GET");
    assert_eq!(routes[0].path(), "/:id");
    assert_eq!(routes[0].summary(), Some("Find user by ID"));
    assert_eq!(routes[0].guards(), ["AuthGuard"]);
    assert_eq!(routes[0].pipes(), ["ValidationPipe"]);
    assert_eq!(routes[1].method(), "POST");
    assert_eq!(routes[1].path(), "/");
    assert_eq!(routes[1].summary(), None);
    assert!(routes[1].guards().is_empty());
    assert!(routes[1].pipes().is_empty());
}

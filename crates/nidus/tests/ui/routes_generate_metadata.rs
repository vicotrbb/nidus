use nidus::prelude::*;

#[controller("/users")]
struct UsersController;
struct AuthGuard;
struct ValidationPipe;
struct CreateUserDto;
struct UserDto;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[guard(AuthGuard)]
    #[pipe(ValidationPipe)]
    #[openapi(
        summary = "Find user by ID",
        tags = ["users", "read"],
        request = CreateUserDto,
        response = UserDto
    )]
    async fn find_one(&self) {}

    #[post("/")]
    #[validate]
    async fn create(&self) {}
}

fn main() {
    let routes = UsersController::routes();
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].method(), "GET");
    assert_eq!(routes[0].path(), "/:id");
    assert_eq!(routes[0].summary(), Some("Find user by ID"));
    assert_eq!(routes[0].tags(), ["users", "read"]);
    assert_eq!(routes[0].request_schema(), Some("CreateUserDto"));
    assert_eq!(routes[0].response_schema(), Some("UserDto"));
    assert_eq!(routes[0].guards(), ["AuthGuard"]);
    assert_eq!(routes[0].pipes(), ["ValidationPipe"]);
    assert!(!routes[0].validates());
    assert_eq!(routes[1].method(), "POST");
    assert_eq!(routes[1].path(), "/");
    assert_eq!(routes[1].summary(), None);
    assert!(routes[1].tags().is_empty());
    assert_eq!(routes[1].request_schema(), None);
    assert_eq!(routes[1].response_schema(), None);
    assert!(routes[1].guards().is_empty());
    assert!(routes[1].pipes().is_empty());
    assert!(routes[1].validates());
}

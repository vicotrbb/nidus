use super::discover_controller_routes;

#[test]
fn discovers_routes_from_syn_attributes_in_controller_impls() {
    let file = syn::parse_file(
        r#"
use nidus::prelude::*;

#[controller("/users")]
pub struct UsersController;

#[routes]
impl UsersController {
    #[guard(crate::auth::AuthGuard)]
    #[pipe(ValidationPipe)]
    #[validate]
    #[openapi(
        summary = "Find user",
        tags = ["users", "read"],
        status = 201,
        request = CreateUserDto,
        response = UserDto
    )]
    #[get(
        "/:id"
    )]
    pub async fn find(&self) {}
}
"#,
    )
    .unwrap();

    let routes = discover_controller_routes(&file).unwrap();

    assert_eq!(routes.len(), 1);
    let route = &routes[0];
    assert_eq!(route.method, "get");
    assert_eq!(route.path, "/users/{id}");
    assert_eq!(route.summary.as_deref(), Some("Find user"));
    assert_eq!(route.tags, ["users", "read"]);
    assert_eq!(route.response_status, Some(201));
    assert_eq!(route.request_schema.as_deref(), Some("CreateUserDto"));
    assert_eq!(route.response_schema.as_deref(), Some("UserDto"));
    assert_eq!(route.guards, ["crate::auth::AuthGuard"]);
    assert_eq!(route.pipes, ["ValidationPipe"]);
    assert!(route.validates);
}

#[test]
fn rejects_duplicate_route_method_attributes() {
    let file = syn::parse_file(
        r#"
use nidus::prelude::*;

#[controller("/users")]
pub struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[post("/")]
    pub async fn find(&self) {}
}
"#,
    )
    .unwrap();

    let error = discover_controller_routes(&file).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("route methods must declare exactly one HTTP method attribute")
    );
}

#[test]
fn rejects_malformed_controller_metadata() {
    let file = syn::parse_file(
        r#"
use nidus::prelude::*;

#[controller]
pub struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    pub async fn find(&self) {}
}
"#,
    )
    .unwrap();

    let error = discover_controller_routes(&file).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("#[controller] requires a string literal path")
    );
}

#[test]
fn rejects_malformed_route_type_metadata() {
    let file = syn::parse_file(
        r#"
use nidus::prelude::*;

#[controller("/users")]
pub struct UsersController;

#[routes]
impl UsersController {
    #[guard]
    #[get("/:id")]
    pub async fn find(&self) {}
}
"#,
    )
    .unwrap();

    let error = discover_controller_routes(&file).unwrap_err();

    assert!(error.to_string().contains("#[guard] requires a type path"));
}

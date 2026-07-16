use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::{discover_controller_routes, discover_routes};

static TEMP_PROJECT_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

struct TempProject(std::path::PathBuf);

impl TempProject {
    fn new() -> Self {
        let sequence = TEMP_PROJECT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "cargo-nidus-routes-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        Self(root)
    }

    fn write(&self, relative: &str, contents: &str) {
        fs::write(self.0.join(relative), contents).unwrap();
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn discovers_controller_and_routes_impl_split_across_source_files() {
    let project = TempProject::new();
    project.write(
        "src/users_controller.rs",
        r#"
#[controller("/users")]
pub struct UsersController;
"#,
    );
    project.write(
        "src/users_routes.rs",
        r#"
#[routes]
impl UsersController {
    #[get("/:id")]
    pub async fn find(&self) {}
}
"#,
    );

    let routes = discover_routes(&project.0).unwrap();

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].method, "get");
    assert_eq!(routes[0].path, "/users/{id}");
}

#[test]
fn reports_ambiguous_cross_file_controller_names() {
    let project = TempProject::new();
    project.write(
        "src/admin_controller.rs",
        r#"
#[controller("/admin/users")]
pub struct UsersController;
"#,
    );
    project.write(
        "src/public_controller.rs",
        r#"
#[controller("/users")]
pub struct UsersController;
"#,
    );
    project.write(
        "src/users_routes.rs",
        r#"
#[routes]
impl UsersController {
    #[get("/:id")]
    pub async fn find(&self) {}
}
"#,
    );

    let error = discover_routes(&project.0).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("ambiguous cross-file controller `UsersController`")
    );
}

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

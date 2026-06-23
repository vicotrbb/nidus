use nidus::prelude::*;
use nidus_openapi::OpenApiDocument;

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct UserDto {
    id: i32,
    email: String,
}

#[controller("/users")]
struct UsersController;

#[routes]
#[allow(dead_code)]
impl UsersController {
    #[get("/:id")]
    #[openapi(summary = "Find user")]
    async fn find_one(&self) {}
}

fn main() {
    let document = OpenApiDocument::from_controller_routes(
        "Nidus Example",
        "0.1.0",
        UsersController::controller_prefix(),
        &UsersController::routes(),
    )
    .schema::<UserDto>();
    println!("{}", document.to_json_value());
}

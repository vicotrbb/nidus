use nidus_openapi::{OpenApiDocument, OpenApiRoute};

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct UserDto {
    id: i32,
    email: String,
}

fn main() {
    let document = OpenApiDocument::new("Nidus Example", "0.1.0")
        .schema::<UserDto>()
        .route(
            OpenApiRoute::get("/users/{id}")
                .summary("Find user")
                .response_schema::<UserDto>(),
        );
    println!("{}", document.to_json_value());
}

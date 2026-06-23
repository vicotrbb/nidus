use nidus_openapi::{OpenApiDocument, OpenApiRoute};

fn main() {
    let document = OpenApiDocument::new("Nidus Example", "0.1.0")
        .route(OpenApiRoute::get("/users/{id}").summary("Find user"));
    println!("{}", document.to_json_value());
}

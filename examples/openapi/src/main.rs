//! OpenAPI document generation and serving from controller route metadata.

use nidus::prelude::*;
use nidus_openapi::OpenApiDocument;
use serde::{Deserialize, Serialize};

#[derive(Serialize, utoipa::ToSchema)]
#[allow(dead_code)]
struct UserDto {
    id: i32,
    email: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
struct CreateUserDto {
    email: String,
}

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[openapi(
        summary = "Find user by ID",
        tags = ["users"],
        status = 200,
        response = UserDto
    )]
    async fn find_one(&self, Path(id): Path<i32>) -> Json<UserDto> {
        Json(UserDto {
            id,
            email: "user@nidus.dev".to_owned(),
        })
    }

    #[post("/")]
    #[openapi(
        summary = "Create user",
        tags = ["users"],
        status = 201,
        request = CreateUserDto,
        response = UserDto
    )]
    async fn create(&self, Json(input): Json<CreateUserDto>) -> (StatusCode, Json<UserDto>) {
        (
            StatusCode::CREATED,
            Json(UserDto {
                id: 1,
                email: input.email,
            }),
        )
    }
}

fn document() -> OpenApiDocument {
    OpenApiDocument::from_controller_routes(
        "Nidus Example API",
        "0.1.0",
        UsersController::controller_prefix(),
        &UsersController::routes(),
    )
    .schema::<CreateUserDto>()
    .schema::<UserDto>()
}

fn app() -> Router {
    UsersController
        .into_router()
        .merge(document().into_router())
}

fn main() {
    let _router = app();
    println!("{}", document().to_json_value());
}

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;
    use serde_json::json;

    #[test]
    fn openapi_metadata_comes_from_route_macros() {
        let routes = UsersController::routes();

        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].method(), "GET");
        assert_eq!(routes[0].path(), "/:id");
        assert_eq!(routes[0].summary(), Some("Find user by ID"));
        assert_eq!(routes[0].tags(), ["users"]);
        assert_eq!(routes[0].response_schema(), Some("UserDto"));
        assert_eq!(routes[1].method(), "POST");
        assert_eq!(routes[1].path(), "/");
        assert_eq!(routes[1].response_status(), Some(StatusCode::CREATED));
        assert_eq!(routes[1].request_schema(), Some("CreateUserDto"));
        assert_eq!(routes[1].response_schema(), Some("UserDto"));
    }

    #[test]
    fn document_includes_paths_and_dto_schemas() {
        let json = document().to_json_value();

        assert_eq!(json["info"]["title"], "Nidus Example API");
        assert_eq!(
            json["paths"]["/users/{id}"]["get"]["summary"],
            "Find user by ID"
        );
        assert_eq!(
            json["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/CreateUserDto"
        );
        assert_eq!(
            json["paths"]["/users"]["post"]["responses"]["201"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/UserDto"
        );
        assert!(json["components"]["schemas"]["CreateUserDto"].is_object());
        assert!(json["components"]["schemas"]["UserDto"].is_object());
    }

    #[tokio::test]
    async fn docs_routes_serve_openapi_json_and_swagger_ui() {
        let app = TestApp::from_router(app());

        app.get("/openapi.json")
            .send()
            .await
            .assert_json(json!({
                "openapi": "3.1.0",
                "info": {
                    "title": "Nidus Example API",
                    "version": "0.1.0"
                },
                "paths": {
                    "/users": {
                        "post": {
                            "operationId": "post_users",
                            "summary": "Create user",
                            "tags": ["users"],
                            "requestBody": {
                                "required": true,
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "$ref": "#/components/schemas/CreateUserDto"
                                        }
                                    }
                                }
                            },
                            "responses": {
                                "201": {
                                    "description": "Success",
                                    "content": {
                                        "application/json": {
                                            "schema": {
                                                "$ref": "#/components/schemas/UserDto"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "/users/{id}": {
                        "get": {
                            "operationId": "get_users_by_id",
                            "summary": "Find user by ID",
                            "tags": ["users"],
                            "parameters": [
                                {
                                    "name": "id",
                                    "in": "path",
                                    "required": true,
                                    "schema": {
                                        "type": "string"
                                    }
                                }
                            ],
                            "responses": {
                                "200": {
                                    "description": "Success",
                                    "content": {
                                        "application/json": {
                                            "schema": {
                                                "$ref": "#/components/schemas/UserDto"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "components": {
                    "schemas": {
                        "CreateUserDto": {
                            "type": "object",
                            "required": ["email"],
                            "properties": {
                                "email": {
                                    "type": "string"
                                }
                            }
                        },
                        "UserDto": {
                            "type": "object",
                            "required": ["id", "email"],
                            "properties": {
                                "email": {
                                    "type": "string"
                                },
                                "id": {
                                    "format": "int32",
                                    "type": "integer"
                                }
                            }
                        }
                    }
                }
            }))
            .await;

        let docs = app.get("/docs").send().await;

        docs.assert_status(StatusCode::OK);
        let html = String::from_utf8(docs.body().to_vec()).unwrap();
        assert!(html.contains("<title>Nidus Example API Documentation</title>"));
        assert!(html.contains("url: \"/openapi.json\""));
    }

    #[tokio::test]
    async fn controller_routes_are_executable() {
        let app = TestApp::from_router(app());

        app.get("/users/42")
            .send()
            .await
            .assert_json(json!({
                "id": 42,
                "email": "user@nidus.dev"
            }))
            .await;

        app.post("/users")
            .json(&json!({
                "email": "new@nidus.dev"
            }))
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "email": "new@nidus.dev"
            }))
            .await;
    }
}

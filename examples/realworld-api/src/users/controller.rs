use nidus::prelude::{
    Json, Path, StatusCode, ValidatedJson, controller, get, openapi, post, routes, validate,
};

use crate::users::{CreateUserDto, UserDto, UsersService};

#[controller("/users")]
pub struct UsersController {
    service: nidus::prelude::Inject<UsersService>,
}

#[routes]
impl UsersController {
    #[post("/")]
    #[validate]
    #[openapi(
        summary = "Create user",
        tags = ["users"],
        status = 201,
        request = CreateUserDto,
        response = UserDto
    )]
    pub async fn create_user(
        &self,
        ValidatedJson(input): ValidatedJson<CreateUserDto>,
    ) -> Result<(StatusCode, Json<UserDto>), nidus::prelude::HttpError> {
        let user = self.service.create_user(input).await?;
        tracing::info!(user.id = user.id, "created user");
        Ok((StatusCode::CREATED, Json(user)))
    }

    #[get("/:id")]
    #[openapi(
        summary = "Find user by ID",
        tags = ["users"],
        status = 200,
        response = UserDto
    )]
    pub async fn find_user(
        &self,
        Path(id): Path<i64>,
    ) -> Result<Json<UserDto>, nidus::prelude::HttpError> {
        Ok(Json(self.service.find_user(id).await?))
    }
}

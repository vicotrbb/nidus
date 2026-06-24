use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateUserDto {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 1, max = 80))]
    pub display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserDto {
    pub id: i64,
    pub email: String,
    pub display_name: String,
}

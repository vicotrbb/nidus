use garde::Validate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateUserDto {
    #[garde(email)]
    pub email: String,
    #[garde(length(min = 1, max = 80))]
    pub display_name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserDto {
    pub id: i64,
    pub email: String,
    pub display_name: String,
}

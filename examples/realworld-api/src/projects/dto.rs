use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateProjectDto {
    pub owner_id: i64,
    #[validate(length(min = 1, max = 120))]
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectDto {
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
}

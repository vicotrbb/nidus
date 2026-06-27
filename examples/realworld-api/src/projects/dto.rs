use garde::Validate;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema, Validate)]
#[garde(allow_unvalidated)]
pub struct CreateProjectDto {
    pub owner_id: i64,
    #[garde(length(min = 1, max = 120))]
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectDto {
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
}

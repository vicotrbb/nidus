use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct CreateTaskDto {
    #[validate(length(min = 1, max = 160))]
    pub title: String,
    #[validate(length(max = 500))]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListTasksQuery {
    pub completed: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TaskDto {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub completed: bool,
}

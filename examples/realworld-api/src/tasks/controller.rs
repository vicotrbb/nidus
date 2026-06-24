use nidus::prelude::{Json, Path, controller, guard, openapi, patch, routes};

use crate::tasks::{TaskDto, TasksService};

#[allow(unused_imports)]
use crate::auth::guard::ApiKeyGuard;

#[controller("/tasks")]
pub struct TasksController {
    service: nidus::prelude::Inject<TasksService>,
}

#[routes]
impl TasksController {
    #[patch("/:id/complete")]
    #[guard(ApiKeyGuard)]
    #[openapi(
        summary = "Complete task",
        tags = ["tasks"],
        status = 200,
        response = TaskDto
    )]
    pub async fn complete_task(
        &self,
        Path(id): Path<i64>,
    ) -> Result<Json<TaskDto>, nidus::prelude::HttpError> {
        Ok(Json(self.service.complete_task(id).await?))
    }
}

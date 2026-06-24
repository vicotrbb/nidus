use nidus::prelude::{
    Json, Path, Query, StatusCode, ValidatedJson, controller, get, guard, openapi, post, routes,
    validate,
};

use crate::{
    projects::{CreateProjectDto, ProjectDto, ProjectsService},
    tasks::{CreateTaskDto, ListTasksQuery, TaskDto, TasksService},
};

#[allow(unused_imports)]
use crate::auth::guard::ApiKeyGuard;

#[controller("/projects")]
pub struct ProjectsController {
    projects: nidus::prelude::Inject<ProjectsService>,
    tasks: nidus::prelude::Inject<TasksService>,
}

#[routes]
impl ProjectsController {
    #[post("/")]
    #[guard(ApiKeyGuard)]
    #[validate]
    #[openapi(
        summary = "Create project",
        tags = ["projects"],
        status = 201,
        request = CreateProjectDto,
        response = ProjectDto
    )]
    pub async fn create_project(
        &self,
        ValidatedJson(input): ValidatedJson<CreateProjectDto>,
    ) -> Result<(StatusCode, Json<ProjectDto>), nidus::prelude::HttpError> {
        let project = self.projects.create_project(input).await?;
        tracing::info!(project.id = project.id, "created project");
        Ok((StatusCode::CREATED, Json(project)))
    }

    #[get("/:id")]
    #[guard(ApiKeyGuard)]
    #[openapi(
        summary = "Find project by ID",
        tags = ["projects"],
        status = 200,
        response = ProjectDto
    )]
    pub async fn find_project(
        &self,
        Path(id): Path<i64>,
    ) -> Result<Json<ProjectDto>, nidus::prelude::HttpError> {
        Ok(Json(self.projects.find_project(id).await?))
    }

    #[post("/:project_id/tasks")]
    #[guard(ApiKeyGuard)]
    #[validate]
    #[openapi(
        summary = "Create task",
        tags = ["tasks"],
        status = 201,
        request = CreateTaskDto,
        response = TaskDto
    )]
    pub async fn create_task(
        &self,
        Path(project_id): Path<i64>,
        ValidatedJson(input): ValidatedJson<CreateTaskDto>,
    ) -> Result<(StatusCode, Json<TaskDto>), nidus::prelude::HttpError> {
        let task = self.tasks.create_task(project_id, input).await?;
        tracing::info!(task.id = task.id, project.id = project_id, "created task");
        Ok((StatusCode::CREATED, Json(task)))
    }

    #[get("/:project_id/tasks")]
    #[guard(ApiKeyGuard)]
    #[openapi(
        summary = "List project tasks",
        tags = ["tasks"],
        status = 200,
        response = TaskDto
    )]
    pub async fn list_tasks(
        &self,
        Path(project_id): Path<i64>,
        Query(query): Query<ListTasksQuery>,
    ) -> Result<Json<Vec<TaskDto>>, nidus::prelude::HttpError> {
        Ok(Json(
            self.tasks.list_project_tasks(project_id, query).await?,
        ))
    }
}

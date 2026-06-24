use nidus::prelude::{HttpError, Inject, injectable};

use crate::{
    db::map_db_error,
    projects::ProjectsService,
    tasks::{CreateTaskDto, ListTasksQuery, TaskDto, TasksRepository, repository::TaskRecord},
};

#[injectable]
#[derive(Debug)]
pub struct TasksService {
    repository: Inject<TasksRepository>,
    projects: Inject<ProjectsService>,
}

impl TasksService {
    pub async fn create_task(
        &self,
        project_id: i64,
        input: CreateTaskDto,
    ) -> Result<TaskDto, HttpError> {
        self.projects.find_project(project_id).await?;
        self.repository
            .create(project_id, input)
            .await
            .map(task_dto)
            .map_err(map_db_error)
    }

    pub async fn list_project_tasks(
        &self,
        project_id: i64,
        query: ListTasksQuery,
    ) -> Result<Vec<TaskDto>, HttpError> {
        self.projects.find_project(project_id).await?;
        self.repository
            .list(project_id, query.completed)
            .await
            .map(|tasks| tasks.into_iter().map(task_dto).collect())
            .map_err(map_db_error)
    }

    pub async fn complete_task(&self, id: i64) -> Result<TaskDto, HttpError> {
        self.repository
            .complete(id)
            .await
            .map_err(map_db_error)?
            .map(task_dto)
            .ok_or_else(|| HttpError::not_found("task not found"))
    }
}

fn task_dto(record: TaskRecord) -> TaskDto {
    TaskDto {
        id: record.id,
        project_id: record.project_id,
        title: record.title,
        description: record.description,
        completed: record.completed,
    }
}

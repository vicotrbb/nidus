use nidus::prelude::{HttpError, Inject, injectable};

use crate::{
    db::map_db_error,
    projects::{CreateProjectDto, ProjectDto, ProjectsRepository, repository::ProjectRecord},
    users::UsersService,
};

#[injectable]
#[derive(Debug)]
pub struct ProjectsService {
    repository: Inject<ProjectsRepository>,
    users: Inject<UsersService>,
}

impl ProjectsService {
    pub async fn create_project(&self, input: CreateProjectDto) -> Result<ProjectDto, HttpError> {
        self.users.find_user(input.owner_id).await?;
        self.repository
            .create(input)
            .await
            .map(project_dto)
            .map_err(map_db_error)
    }

    pub async fn find_project(&self, id: i64) -> Result<ProjectDto, HttpError> {
        self.repository
            .find_by_id(id)
            .await
            .map_err(map_db_error)?
            .map(project_dto)
            .ok_or_else(|| HttpError::not_found("project not found"))
    }
}

fn project_dto(record: ProjectRecord) -> ProjectDto {
    ProjectDto {
        id: record.id,
        owner_id: record.owner_id,
        name: record.name,
    }
}

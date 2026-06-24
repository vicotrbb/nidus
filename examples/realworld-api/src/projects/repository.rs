use nidus::prelude::{Inject, injectable};

use crate::{db::Database, projects::CreateProjectDto};

#[derive(Debug)]
pub struct ProjectRecord {
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
}

#[injectable]
#[derive(Debug)]
pub struct ProjectsRepository {
    database: Inject<Database>,
}

impl ProjectsRepository {
    pub async fn create(&self, input: CreateProjectDto) -> sqlx::Result<ProjectRecord> {
        let result = sqlx::query("INSERT INTO projects (owner_id, name) VALUES (?1, ?2)")
            .bind(input.owner_id)
            .bind(&input.name)
            .execute(self.database.pool())
            .await?;

        Ok(ProjectRecord {
            id: result.last_insert_rowid(),
            owner_id: input.owner_id,
            name: input.name,
        })
    }

    pub async fn find_by_id(&self, id: i64) -> sqlx::Result<Option<ProjectRecord>> {
        let row = sqlx::query_as::<_, (i64, i64, String)>(
            "SELECT id, owner_id, name FROM projects WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(|(id, owner_id, name)| ProjectRecord { id, owner_id, name }))
    }
}

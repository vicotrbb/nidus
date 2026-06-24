use nidus::prelude::{Inject, injectable};

use crate::{db::Database, tasks::CreateTaskDto};

#[derive(Debug)]
pub struct TaskRecord {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub completed: bool,
}

#[injectable]
#[derive(Debug)]
pub struct TasksRepository {
    database: Inject<Database>,
}

impl TasksRepository {
    pub async fn create(&self, project_id: i64, input: CreateTaskDto) -> sqlx::Result<TaskRecord> {
        let result = sqlx::query(
            "INSERT INTO tasks (project_id, title, description, completed) VALUES (?1, ?2, ?3, 0)",
        )
        .bind(project_id)
        .bind(&input.title)
        .bind(&input.description)
        .execute(self.database.pool())
        .await?;

        Ok(TaskRecord {
            id: result.last_insert_rowid(),
            project_id,
            title: input.title,
            description: input.description,
            completed: false,
        })
    }

    pub async fn list(
        &self,
        project_id: i64,
        completed: Option<bool>,
    ) -> sqlx::Result<Vec<TaskRecord>> {
        let rows = if let Some(completed) = completed {
            sqlx::query_as::<_, (i64, i64, String, Option<String>, bool)>(
                "SELECT id, project_id, title, description, completed FROM tasks WHERE project_id = ?1 AND completed = ?2 ORDER BY id",
            )
            .bind(project_id)
            .bind(completed)
            .fetch_all(self.database.pool())
            .await?
        } else {
            sqlx::query_as::<_, (i64, i64, String, Option<String>, bool)>(
                "SELECT id, project_id, title, description, completed FROM tasks WHERE project_id = ?1 ORDER BY id",
            )
            .bind(project_id)
            .fetch_all(self.database.pool())
            .await?
        };

        Ok(rows
            .into_iter()
            .map(
                |(id, project_id, title, description, completed)| TaskRecord {
                    id,
                    project_id,
                    title,
                    description,
                    completed,
                },
            )
            .collect())
    }

    pub async fn complete(&self, id: i64) -> sqlx::Result<Option<TaskRecord>> {
        sqlx::query("UPDATE tasks SET completed = 1 WHERE id = ?1")
            .bind(id)
            .execute(self.database.pool())
            .await?;

        self.find_by_id(id).await
    }

    async fn find_by_id(&self, id: i64) -> sqlx::Result<Option<TaskRecord>> {
        let row = sqlx::query_as::<_, (i64, i64, String, Option<String>, bool)>(
            "SELECT id, project_id, title, description, completed FROM tasks WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(
            |(id, project_id, title, description, completed)| TaskRecord {
                id,
                project_id,
                title,
                description,
                completed,
            },
        ))
    }
}

use nidus::prelude::{Inject, injectable};

use crate::{db::Database, users::CreateUserDto};

#[derive(Debug)]
pub struct UserRecord {
    pub id: i64,
    pub email: String,
    pub display_name: String,
}

#[injectable]
#[derive(Debug)]
pub struct UsersRepository {
    database: Inject<Database>,
}

impl UsersRepository {
    pub async fn create(&self, input: CreateUserDto) -> sqlx::Result<UserRecord> {
        let result = sqlx::query("INSERT INTO users (email, display_name) VALUES (?1, ?2)")
            .bind(&input.email)
            .bind(&input.display_name)
            .execute(self.database.pool())
            .await?;

        Ok(UserRecord {
            id: result.last_insert_rowid(),
            email: input.email,
            display_name: input.display_name,
        })
    }

    pub async fn find_by_id(&self, id: i64) -> sqlx::Result<Option<UserRecord>> {
        let row = sqlx::query_as::<_, (i64, String, String)>(
            "SELECT id, email, display_name FROM users WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await?;

        Ok(row.map(|(id, email, display_name)| UserRecord {
            id,
            email,
            display_name,
        }))
    }
}

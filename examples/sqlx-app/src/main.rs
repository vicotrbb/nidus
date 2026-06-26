use nidus::prelude::{Container, Inject};
use nidus_sqlx::SqlitePoolProvider;

struct UsersRepository {
    database: Inject<SqlitePoolProvider>,
}

impl UsersRepository {
    fn new(database: Inject<SqlitePoolProvider>) -> Self {
        Self { database }
    }

    async fn count_users(&self) -> sqlx::Result<i64> {
        let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users")
            .fetch_one(self.database.pool())
            .await?;
        Ok(count)
    }
}

async fn build_container() -> nidus::prelude::Result<Container> {
    let mut container = Container::new();
    SqlitePoolProvider::builder()
        .database_url("sqlite::memory:")
        .max_connections(1)
        .register(&mut container)
        .await
        .map_err(|error| nidus::prelude::NidusError::ApplicationBuild {
            message: error.to_string(),
        })?;

    let provider = container.resolve::<SqlitePoolProvider>()?;
    sqlx::query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(provider.pool())
        .await
        .map_err(|error| nidus::prelude::NidusError::ApplicationBuild {
            message: error.to_string(),
        })?;

    container.register_singleton_factory(|container| {
        Ok(UsersRepository::new(
            container.inject::<SqlitePoolProvider>()?,
        ))
    })?;
    Ok(container)
}

#[tokio::main]
async fn main() -> nidus::prelude::Result<()> {
    let container = build_container().await?;
    let repository = container.resolve::<UsersRepository>()?;
    let _count = repository.count_users().await.map_err(|error| {
        nidus::prelude::NidusError::ApplicationBuild {
            message: error.to_string(),
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn example_wires_sqlite_provider() {
        let container = build_container().await.unwrap();
        assert!(container.resolve::<SqlitePoolProvider>().is_ok());
        assert_eq!(
            container
                .resolve::<UsersRepository>()
                .unwrap()
                .count_users()
                .await
                .unwrap(),
            0
        );
    }
}

use nidus::prelude::{HttpError, Inject, injectable};

use crate::{
    db::map_db_error,
    users::{CreateUserDto, UserDto, UsersRepository, repository::UserRecord},
};

#[injectable]
#[derive(Debug)]
pub struct UsersService {
    repository: Inject<UsersRepository>,
}

impl UsersService {
    pub async fn create_user(&self, input: CreateUserDto) -> Result<UserDto, HttpError> {
        self.repository
            .create(input)
            .await
            .map(user_dto)
            .map_err(map_db_error)
    }

    pub async fn find_user(&self, id: i64) -> Result<UserDto, HttpError> {
        self.repository
            .find_by_id(id)
            .await
            .map_err(map_db_error)?
            .map(user_dto)
            .ok_or_else(|| HttpError::not_found("user not found"))
    }
}

fn user_dto(record: UserRecord) -> UserDto {
    UserDto {
        id: record.id,
        email: record.email,
        display_name: record.display_name,
    }
}

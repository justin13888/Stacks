use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};

use crate::entity::user;

use super::{UserError, repo};

pub struct UserService {
    db: DatabaseConnection,
}

impl UserService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_by_id(&self, id: &str) -> Result<user::Model, UserError> {
        repo::find_by_id(&self.db, id)
            .await
            .map_err(UserError::Db)?
            .ok_or(UserError::NotFound)
    }

    pub async fn list(&self) -> Result<Vec<user::Model>, UserError> {
        repo::list(&self.db).await
    }

    pub async fn update_settings(
        &self,
        user_id: &str,
        settings: serde_json::Value,
    ) -> Result<user::Model, UserError> {
        let user = self.get_by_id(user_id).await?;
        let mut active: user::ActiveModel = user.into();
        active.settings = Set(settings);
        active.updated_at = Set(Utc::now().into());
        repo::update(&self.db, active).await
    }

    pub async fn update_username(
        &self,
        user_id: &str,
        username: String,
    ) -> Result<user::Model, UserError> {
        let user = self.get_by_id(user_id).await?;
        let mut active: user::ActiveModel = user.into();
        active.username = Set(username);
        active.updated_at = Set(Utc::now().into());
        repo::update(&self.db, active).await
    }

    pub async fn update_role(&self, user_id: &str, role: String) -> Result<user::Model, UserError> {
        let user = self.get_by_id(user_id).await?;
        let mut active: user::ActiveModel = user.into();
        active.role = Set(role);
        active.updated_at = Set(Utc::now().into());
        repo::update(&self.db, active).await
    }

    pub async fn delete(&self, user_id: &str) -> Result<bool, UserError> {
        repo::delete(&self.db, user_id).await
    }
}

use sea_orm::ActiveValue::{Set, Unchanged};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::db::now;
use crate::entity::enums::Role;
use crate::entity::user;

use super::settings::UserSettings;
use super::{UserError, repo};

pub struct UserService {
    db: DatabaseConnection,
}

impl UserService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<user::Model, UserError> {
        repo::find_by_id(&self.db, id)
            .await?
            .ok_or(UserError::NotFound)
    }

    pub async fn list(&self) -> Result<Vec<user::Model>, UserError> {
        Ok(repo::list(&self.db).await?)
    }

    /// Merge `patch` (a JSON object) into the stored settings: keys present
    /// replace the stored value, a `null` removes the key (back to its
    /// default), and keys not mentioned are left alone. The result must still
    /// be valid [`UserSettings`].
    pub async fn update_settings(
        &self,
        user_id: Uuid,
        patch: serde_json::Value,
    ) -> Result<user::Model, UserError> {
        let user = self.get_by_id(user_id).await?;
        let serde_json::Value::Object(patch) = patch else {
            return Err(UserError::InvalidSettings(
                "settings must be an object".into(),
            ));
        };
        let mut merged = match user.settings.clone() {
            serde_json::Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        for (k, v) in patch {
            if v.is_null() {
                merged.remove(&k);
            } else {
                merged.insert(k, v);
            }
        }
        let merged = serde_json::Value::Object(merged);
        UserSettings::from_json(&merged).map_err(|e| UserError::InvalidSettings(e.to_string()))?;
        self.write(user.id, |am| am.settings = Set(merged)).await
    }

    pub async fn update_display_name(
        &self,
        user_id: Uuid,
        display_name: String,
    ) -> Result<user::Model, UserError> {
        if display_name.trim().is_empty() {
            return Err(UserError::EmptyDisplayName);
        }
        self.get_by_id(user_id).await?;
        self.write(user_id, |am| am.display_name = Set(display_name))
            .await
    }

    pub async fn update_role(&self, user_id: Uuid, role: Role) -> Result<user::Model, UserError> {
        self.get_by_id(user_id).await?;
        self.write(user_id, |am| am.role = Set(role)).await
    }

    /// Stop a user signing in. Their data is untouched and
    /// [`reactivate`](Self::reactivate) undoes it. This is what "delete a
    /// user" means everywhere outside the explicit [`super::purge`].
    pub async fn deactivate(&self, user_id: Uuid) -> Result<user::Model, UserError> {
        let user = self.get_by_id(user_id).await?;
        if user.deactivated_at.is_some() {
            return Ok(user);
        }
        self.write(user_id, |am| am.deactivated_at = Set(Some(now())))
            .await
    }

    pub async fn reactivate(&self, user_id: Uuid) -> Result<user::Model, UserError> {
        self.get_by_id(user_id).await?;
        self.write(user_id, |am| am.deactivated_at = Set(None))
            .await
    }

    async fn write(
        &self,
        user_id: Uuid,
        f: impl FnOnce(&mut user::ActiveModel),
    ) -> Result<user::Model, UserError> {
        let mut am = user::ActiveModel {
            id: Unchanged(user_id),
            ..Default::default()
        };
        f(&mut am);
        Ok(repo::update(&self.db, am).await?)
    }
}

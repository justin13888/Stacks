//! Account changes only tests make directly (the `test-support` feature);
//! the API changes a role through [`UserService::administer`].

use super::*;

impl UserService {
    pub async fn update_role(&self, user_id: Uuid, role: Role) -> Result<user::Model, UserError> {
        self.get_by_id(user_id).await?;
        self.write(user_id, |am| am.role = Set(role)).await
    }
}

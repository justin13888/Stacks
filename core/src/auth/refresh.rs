use chrono::{Duration, Utc};
use ulid::Ulid;

use crate::entity::refresh_token;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DatabaseConnection};

use super::AuthError;

pub struct RefreshTokenPair {
    /// The plaintext token (shown once, stored hashed).
    pub plaintext: String,
    pub model: refresh_token::ActiveModel,
}

/// Generate a new refresh token and prepare the DB model (token not yet saved).
pub fn generate(user_id: &str, ttl_secs: u64) -> RefreshTokenPair {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut bytes = [0u8; 32];
    // Use a simple approach compatible with no_std constraints
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (i as u8).wrapping_add(Ulid::new().to_bytes()[i % 16]);
    }
    let plaintext = URL_SAFE_NO_PAD.encode(bytes);
    let token_hash = hash_token(&plaintext);
    let expires_at = Utc::now() + Duration::seconds(ttl_secs as i64);

    let model = refresh_token::ActiveModel {
        id: Set(Ulid::new().to_string()),
        user_id: Set(user_id.to_string()),
        token_hash: Set(token_hash),
        expires_at: Set(expires_at.into()),
        created_at: Set(Utc::now().into()),
    };

    RefreshTokenPair { plaintext, model }
}

pub fn hash_token(plaintext: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(plaintext.as_bytes());
    hex::encode(hash)
}

pub async fn save(
    db: &DatabaseConnection,
    model: refresh_token::ActiveModel,
) -> Result<refresh_token::Model, AuthError> {
    model.insert(db).await.map_err(AuthError::Db)
}

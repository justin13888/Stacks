use argon2::password_hash::rand_core::{OsRng, RngCore};
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
    // 32 bytes straight from the OS CSPRNG. This previously derived token
    // bytes from `Ulid::new()`, which is 48 bits of *timestamp* plus 80 bits of
    // randomness — largely predictable from the issue time.
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique_and_full_entropy() {
        let a = generate("user-1", 60);
        let b = generate("user-1", 60);
        assert_ne!(a.plaintext, b.plaintext);
        // 32 bytes base64url-no-pad
        assert_eq!(a.plaintext.len(), 43);
    }

    #[test]
    fn hash_is_deterministic_and_differs_per_token() {
        let a = generate("user-1", 60);
        assert_eq!(hash_token(&a.plaintext), hash_token(&a.plaintext));
        assert_ne!(hash_token(&a.plaintext), hash_token("something-else"));
    }
}

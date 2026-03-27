use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use ulid::Ulid;

use super::AuthError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Read,
    Write,
    Admin,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Read => "read",
            Scope::Write => "write",
            Scope::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Scope::Read),
            "write" => Some(Scope::Write),
            "admin" => Some(Scope::Admin),
            _ => None,
        }
    }
}

/// Generate a new opaque API token and return (plaintext, argon2id_hash).
pub fn generate() -> (String, String) {
    let plaintext = format!("stk_{}", Ulid::new());
    let hash = hash_token(&plaintext).expect("hashing should not fail");
    (plaintext, hash)
}

pub fn hash_token(plaintext: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::OidcError(e.to_string()))
}

pub fn verify_token(plaintext: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(plaintext.as_bytes(), &parsed)
        .is_ok()
}

pub fn has_scope(scopes: &[String], required: &Scope) -> bool {
    scopes
        .iter()
        .any(|s| Scope::parse(s).as_ref() == Some(required))
}

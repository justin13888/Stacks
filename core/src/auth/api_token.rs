use argon2::{
    Argon2,
    password_hash::{
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
        rand_core::{OsRng, RngCore},
    },
};

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

/// Generate a new opaque API token.
///
/// Returns `(plaintext, argon2id_hash, lookup_hash)`. The argon2 hash is what
/// authenticates the token; the lookup hash is a plain SHA-256 that exists only
/// so the row can be found with a single indexed query (argon2 is salted and
/// therefore unsearchable).
pub fn generate() -> (String, String, String) {
    // Previously `format!("stk_{}", Ulid::new())` — a ULID leads with 48 bits
    // of timestamp, so a token's prefix was guessable from its creation time.
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let plaintext = format!("stk_{}", hex::encode(bytes));
    let hash = hash_token(&plaintext).expect("hashing should not fail");
    let lookup = lookup_hash(&plaintext);
    (plaintext, hash, lookup)
}

/// Deterministic SHA-256 of a token, used purely as an index key.
pub fn lookup_hash(plaintext: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(plaintext.as_bytes()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique_and_prefixed() {
        let (a, _, _) = generate();
        let (b, _, _) = generate();
        assert!(a.starts_with("stk_"));
        assert_ne!(a, b);
        // "stk_" + 32 bytes hex
        assert_eq!(a.len(), 4 + 64);
    }

    #[test]
    fn hash_verifies_only_the_matching_plaintext() {
        let (plaintext, hash, _) = generate();
        assert!(verify_token(&plaintext, &hash));
        assert!(!verify_token("stk_wrong", &hash));
        assert!(!verify_token(&plaintext, "not-a-hash"));
    }

    #[test]
    fn lookup_hash_is_deterministic() {
        let (plaintext, _, lookup) = generate();
        assert_eq!(lookup_hash(&plaintext), lookup);
        assert_ne!(lookup_hash("stk_other"), lookup);
    }

    #[test]
    fn scope_round_trips_and_rejects_unknown() {
        for s in ["read", "write", "admin"] {
            assert_eq!(Scope::parse(s).unwrap().as_str(), s);
        }
        assert!(Scope::parse("superuser").is_none());
    }

    #[test]
    fn has_scope_matches_only_granted_scopes() {
        let scopes = vec!["read".to_string(), "write".to_string()];
        assert!(has_scope(&scopes, &Scope::Read));
        assert!(has_scope(&scopes, &Scope::Write));
        assert!(!has_scope(&scopes, &Scope::Admin));
    }
}

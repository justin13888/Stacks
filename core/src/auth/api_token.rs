//! Opaque, scoped API tokens (`stk_` + 32 random bytes as hex).
//!
//! Only the SHA-256 of a token is stored. The token carries 256 bits of CSPRNG
//! entropy, so a fast unsalted hash is the right tool: there is nothing to
//! brute-force, and the hash doubles as the unique lookup key (one indexed
//! query per request rather than a salted hash compared row by row).

pub use crate::entity::enums::Scope;

/// Characters of the plaintext kept for display (`stk_` + 4 hex digits).
const PREFIX_LEN: usize = 8;

/// A freshly generated token.
pub struct GeneratedToken {
    /// Shown to the user exactly once.
    pub plaintext: String,
    /// The first characters, stored so the user can recognise the token.
    pub prefix: String,
    /// SHA-256 of the plaintext, hex — the stored lookup key.
    pub lookup_hash: String,
}

pub fn generate() -> GeneratedToken {
    let plaintext = format!("stk_{}", hex::encode(super::random_bytes::<32>()));
    GeneratedToken {
        prefix: plaintext[..PREFIX_LEN].to_owned(),
        lookup_hash: lookup_hash(&plaintext),
        plaintext,
    }
}

/// Deterministic SHA-256 of a token, hex-encoded.
pub fn lookup_hash(plaintext: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(plaintext.as_bytes()))
}

/// Parse requested scopes, rejecting unknown ones and empty sets.
pub fn parse_scopes(raw: &[String]) -> Result<Vec<Scope>, String> {
    if raw.is_empty() {
        return Err("at least one scope is required".into());
    }
    let mut out = Vec::with_capacity(raw.len());
    for s in raw {
        let scope = Scope::parse(s).ok_or_else(|| format!("unknown scope: {s}"))?;
        if !out.contains(&scope) {
            out.push(scope);
        }
    }
    Ok(out)
}

pub fn has_scope(scopes: &[String], required: Scope) -> bool {
    scopes.iter().any(|s| Scope::parse(s) == Some(required))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique_and_prefixed() {
        let a = generate();
        let b = generate();
        assert!(a.plaintext.starts_with("stk_"));
        assert_ne!(a.plaintext, b.plaintext);
        // "stk_" + 32 bytes hex
        assert_eq!(a.plaintext.len(), 4 + 64);
        assert_eq!(a.prefix, &a.plaintext[..8]);
    }

    #[test]
    fn lookup_hash_is_deterministic() {
        let t = generate();
        assert_eq!(lookup_hash(&t.plaintext), t.lookup_hash);
        assert_ne!(lookup_hash("stk_other"), t.lookup_hash);
        assert_eq!(t.lookup_hash.len(), 64);
    }

    #[test]
    fn scope_parsing() {
        let ok = parse_scopes(&["read".into(), "write".into(), "read".into()]).unwrap();
        assert_eq!(ok, vec![Scope::Read, Scope::Write]);
        assert!(parse_scopes(&[]).is_err());
        assert!(parse_scopes(&["superuser".into()]).is_err());
    }

    #[test]
    fn has_scope_matches_only_granted_scopes() {
        let scopes = vec!["read".to_string(), "write".to_string()];
        assert!(has_scope(&scopes, Scope::Read));
        assert!(has_scope(&scopes, Scope::Write));
        assert!(!has_scope(&scopes, Scope::Admin));
    }
}

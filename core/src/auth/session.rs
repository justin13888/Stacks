//! Opaque session tokens. The plaintext goes to the client once; only its
//! SHA-256 is stored in `sessions.token_hash`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// A new session token: `(plaintext, sha256_hex)`.
pub fn generate() -> (String, String) {
    let plaintext = URL_SAFE_NO_PAD.encode(super::random_bytes::<32>());
    let hash = hash_token(&plaintext);
    (plaintext, hash)
}

pub fn hash_token(plaintext: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(plaintext.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique_and_full_entropy() {
        let (a, ha) = generate();
        let (b, _) = generate();
        assert_ne!(a, b);
        // 32 bytes base64url-no-pad
        assert_eq!(a.len(), 43);
        assert_eq!(hash_token(&a), ha);
        assert_eq!(ha.len(), 64);
    }
}

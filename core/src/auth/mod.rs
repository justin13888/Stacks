pub mod api_token;
pub mod error;
pub mod oidc;
pub mod repo;
pub mod service;
pub mod session;

pub use error::AuthError;
pub use service::{AuthService, Authenticated, NewSession, SessionMeta};

/// `N` bytes from the thread-local CSPRNG (ChaCha, seeded from the OS).
pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    bytes
}

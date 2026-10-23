pub mod auth_user;
pub mod pagination;

use salvo::prelude::*;
use stacks_core::Uuid;

/// A UUID path parameter; `None` when absent or malformed, which handlers
/// report as "not found" (no such id can exist).
pub fn uuid_param(req: &Request, name: &str) -> Option<Uuid> {
    req.param::<String>(name)
        .and_then(|s| Uuid::parse_str(&s).ok())
}

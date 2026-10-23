//! The Stacks HTTP API, on kynos.
//!
//! One module per area, each holding its operations, the DTOs they read and
//! write, and the error each can answer with. [`app::router`] assembles them;
//! the OpenAPI 3.1 document is derived from that router and nothing else.

pub mod account;
pub mod app;
pub mod ids;
pub mod limits;
pub mod observe;
pub mod ops;
pub mod page;
pub mod patch;
pub mod problem;
pub mod security;
pub mod sessions;

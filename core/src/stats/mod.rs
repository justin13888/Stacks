pub mod error;
pub mod models;
pub mod repo;
pub mod service;

pub use error::StatsError;
pub use models::{Binge, RatedTitle, Streak, TOP_RATED, UserStats};
pub use service::StatsService;

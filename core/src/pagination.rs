use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct CursorParams {
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

impl CursorParams {
    pub fn limit_or_default(&self) -> u64 {
        self.limit.unwrap_or(50).min(200)
    }
}

#[derive(Debug, Serialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }
}

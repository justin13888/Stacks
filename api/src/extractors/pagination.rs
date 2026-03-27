//! Parse cursor/limit query parameters into CursorParams.

use salvo::prelude::*;
use stacks_core::pagination::CursorParams;

pub fn from_query(req: &Request) -> CursorParams {
    CursorParams {
        cursor: req.query::<String>("cursor"),
        limit: req.query::<u64>("limit"),
    }
}

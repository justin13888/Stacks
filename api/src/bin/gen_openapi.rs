//! Print the public API's OpenAPI 3.1 document.
//!
//! The document is the router's own description: nothing is added to it or
//! patched in it afterwards. `openapi_as(V3_1)` refuses, rather than
//! downgrades, a construct that only OpenAPI 3.2 can describe.
//!
//! ```text
//! cargo run -p stacks-api --bin gen-openapi > web/openapi.json
//! ```
//!
//! `just gen-openapi` writes the committed `web/openapi.json`; CI regenerates
//! it and fails when the two differ.

use std::io::Write as _;
use std::process::ExitCode;

use kynos::openapi::SpecVersion;

fn main() -> ExitCode {
    let router = api::app::router(&api::app::Settings::default());
    let document = match router
        .openapi_as(SpecVersion::V3_1)
        .map_err(|e| e.to_string())
        .and_then(|d| d.to_json().map_err(|e| e.to_string()))
    {
        Ok(document) => document,
        Err(e) => {
            eprintln!("gen-openapi: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = writeln!(stdout, "{document}") {
        eprintln!("gen-openapi: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

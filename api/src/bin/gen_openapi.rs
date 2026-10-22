//! Generates the OpenAPI 3.1.0 spec and writes it to stdout.
//! Usage: cargo run --bin gen-openapi > openapi.json
//!
//! The spec — including the path-parameter back-fill — is built by
//! `api::app::openapi_doc`, so this output is identical to what the running
//! server serves at `/api-docs/openapi.json`.

fn main() {
    let doc = api::app::openapi_json();
    let pretty = serde_json::to_string_pretty(&doc).expect("failed to format OpenAPI spec");
    println!("{pretty}");
}

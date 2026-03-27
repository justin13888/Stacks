//! Generates the OpenAPI 3.1.0 spec and writes it to stdout.
//! Usage: cargo run --bin gen-openapi > openapi.json

fn main() {
    // Build the router (no live DB needed — spec is derived from type signatures)
    let doc = api::app::openapi_doc();
    let json = serde_json::to_string_pretty(&doc).expect("failed to serialize OpenAPI spec");
    println!("{json}");
}

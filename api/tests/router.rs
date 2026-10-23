//! The router and the document it emits, with no database.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kynos::openapi::SpecVersion;
use serde_json::Value;

fn document() -> Value {
    let doc = api::app::router(&api::app::Settings::default())
        .openapi_as(SpecVersion::V3_1)
        .expect("the 3.1 emission succeeds: nothing needs 3.2");
    serde_json::to_value(doc).expect("the document as JSON")
}

#[test]
fn the_router_validates() {
    let violations = api::app::router(&api::app::Settings::default())
        .validate()
        .expect("validation runs");
    assert!(violations.is_empty(), "violations: {violations:#?}");
}

#[test]
fn the_document_is_openapi_3_1() {
    let doc = document();
    let version = doc["openapi"].as_str().expect("an openapi version");
    assert!(version.starts_with("3.1."), "emitted {version}");
}

/// Every operation names itself, says what it does, and belongs to an area.
#[test]
fn every_operation_has_an_id_a_summary_and_a_tag() {
    let doc = document();
    let mut missing = Vec::new();
    for (path, item) in doc["paths"].as_object().expect("paths") {
        for (method, op) in item.as_object().expect("a path item") {
            let Some(op) = op.as_object() else { continue };
            if !op.contains_key("responses") {
                continue;
            }
            for field in ["operationId", "summary"] {
                if op
                    .get(field)
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    missing.push(format!("{method} {path}: {field}"));
                }
            }
            let tagged = op
                .get("tags")
                .and_then(Value::as_array)
                .is_some_and(|t| !t.is_empty());
            let is_docs = path.starts_with("/api/docs") || path == "/api/openapi.json";
            if !tagged && !is_docs {
                missing.push(format!("{method} {path}: tags"));
            }
        }
    }
    assert!(missing.is_empty(), "incomplete operations: {missing:#?}");
}

/// The committed `web/openapi.json` is exactly what the router emits, so the
/// web client and the spargen e2e client are built from the real contract.
#[test]
fn the_committed_document_is_current() {
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/openapi.json"))
            .expect("web/openapi.json is committed");
    let committed: Value = serde_json::from_str(&committed).expect("web/openapi.json is JSON");
    assert!(
        committed == document(),
        "web/openapi.json is stale; regenerate it with `just gen-openapi`"
    );
}

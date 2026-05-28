//! Generates the OpenAPI 3.1.0 spec and writes it to stdout.
//! Usage: cargo run --bin gen-openapi > openapi.json
//!
//! Path parameters in the route template (e.g. `/{id}/retry`) are not declared
//! in every `#[salvo::oapi::endpoint]` macro today, which produces a spec that
//! is technically invalid (and breaks orval). To keep handlers terse without
//! tracking every path placeholder by hand, this binary post-processes the
//! serialized spec and back-fills missing path parameter declarations from
//! each operation's path template.

use std::collections::HashSet;

fn main() {
    let doc = api::app::openapi_doc();
    let mut json: serde_json::Value =
        serde_json::to_value(&doc).expect("failed to serialize OpenAPI spec");
    inject_path_params(&mut json);
    let pretty = serde_json::to_string_pretty(&json).expect("failed to format OpenAPI spec");
    println!("{pretty}");
}

/// Walk every operation in `paths` and ensure each `{name}` placeholder in the
/// URL template has a matching entry in the operation's `parameters` array.
fn inject_path_params(spec: &mut serde_json::Value) {
    let Some(paths) = spec.get_mut("paths").and_then(|p| p.as_object_mut()) else {
        return;
    };
    for (path, item) in paths.iter_mut() {
        let placeholders: Vec<String> = path
            .split('{')
            .skip(1)
            .filter_map(|chunk| chunk.split('}').next().map(str::to_string))
            .filter(|n| !n.is_empty())
            .collect();
        if placeholders.is_empty() {
            continue;
        }
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        for (method, op) in item.iter_mut() {
            if !matches!(
                method.as_str(),
                "get" | "post" | "put" | "delete" | "patch" | "head" | "options"
            ) {
                continue;
            }
            let Some(op) = op.as_object_mut() else {
                continue;
            };
            let params = op
                .entry("parameters".to_string())
                .or_insert_with(|| serde_json::json!([]));
            let Some(arr) = params.as_array_mut() else {
                continue;
            };
            let existing: HashSet<String> = arr
                .iter()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect();
            for name in &placeholders {
                if existing.contains(name) {
                    continue;
                }
                arr.push(serde_json::json!({
                    "name": name,
                    "in": "path",
                    "required": true,
                    "schema": { "type": "string" },
                }));
            }
        }
    }
}

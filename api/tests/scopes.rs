//! Token scopes are enforced on every operation, table-driven from the
//! document: whatever operation an area adds, this checks it.
//!
//! - an operation that changes data refuses a token without `write`;
//! - an operation that reads refuses a token without `read`;
//! - an administrative operation refuses a non-administrator, by session or
//!   token, and an administrator's token without `admin`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use kynos::http::{Method, StatusCode};
use serde_json::Value;
use stacks_core::entity::enums::Role;
use support::Credentials;
use support::example::Examples;

struct Guarded {
    method: Method,
    path: String,
    token_scope: String,
}

fn guarded(document: &Value) -> Vec<Guarded> {
    let examples = Examples::new(document);
    let mut out = Vec::new();
    for (template, item) in document["paths"].as_object().expect("paths") {
        for (method, op) in item.as_object().expect("a path item") {
            let Some(scope) = op["security"].as_array().and_then(|alternatives| {
                alternatives.iter().find_map(|r| {
                    r["ApiToken"]
                        .as_array()?
                        .first()?
                        .as_str()
                        .map(str::to_owned)
                })
            }) else {
                continue;
            };
            let mut path = template.clone();
            for p in op["parameters"].as_array().into_iter().flatten() {
                if p["in"] == "path" {
                    let name = p["name"].as_str().expect("a name");
                    let value = match examples.of(&p["schema"]) {
                        Value::String(s) => s,
                        other => other.to_string(),
                    };
                    path = path.replace(&format!("{{{name}}}"), &value);
                }
            }
            out.push(Guarded {
                method: Method::from_bytes(method.to_ascii_uppercase().as_bytes())
                    .expect("a method"),
                path,
                token_scope: scope,
            });
        }
    }
    out
}

#[tokio::test]
async fn every_operation_enforces_its_token_scope() {
    let env = env!();
    let client = env.client();
    let ada = env.user(Role::User).await;
    let root = env.user(Role::Admin).await;
    let document = serde_json::to_value(&env.document).expect("JSON");
    let ops = guarded(&document);
    assert!(ops.iter().any(|op| op.token_scope == "write"));
    // The one operation that destroys an account is an administrator's alone.
    assert!(
        ops.iter().any(|op| op.method == Method::DELETE
            && op.path.starts_with("/api/v1/admin/users/")
            && op.token_scope == "admin"),
        "purging an account must need `admin`"
    );

    let mut wrong = Vec::new();
    for op in &ops {
        let mut refusals: Vec<(&str, String, bool)> = Vec::new();
        match op.token_scope.as_str() {
            "write" => refusals.push(("a read-only token", ada.read.clone(), true)),
            "read" => refusals.push(("a write-only token", ada.write.clone(), true)),
            "admin" => {
                refusals.push(("a non-administrator's token", ada.read_write.clone(), true));
                refusals.push((
                    "an administrator's token without admin",
                    root.read_write.clone(),
                    true,
                ));
                refusals.push(("a non-administrator's session", String::new(), false));
            }
            other => panic!("an unknown scope `{other}`"),
        }
        for (who, token, bearer) in refusals {
            let request = client.method(op.method.clone(), &op.path);
            let request = if bearer {
                request.bearer(&token)
            } else {
                request.session(&ada)
            };
            let response = request.send().await;
            if response.status() != StatusCode::FORBIDDEN {
                wrong.push(format!(
                    "{} {} with {who}: {}",
                    op.method,
                    op.path,
                    response.status()
                ));
            } else {
                response.assert_problem_type(&support::problem("insufficient-scope"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "scopes not enforced:\n{}",
        wrong.join("\n")
    );
    client.assert_conformance();
}

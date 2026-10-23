//! The statuses every operation shares, driven from the document.
//!
//! For each operation carrying a tag, and for each of these statuses it
//! declares, one request that must produce it:
//!
//! | Status | Request |
//! | --- | --- |
//! | 401 | no credential |
//! | 403 | a deactivated account's session; a cross-site `Sec-Fetch-Site` for an unsafe method |
//! | 406 | `Accept-Encoding` refusing every coding, `identity` included |
//! | 429 | the rate limiter already spent |
//! | 400 | an unparseable path parameter, `limit=0`, malformed JSON, or a missing required query parameter |
//! | 413 | a declared length past the body limit |
//! | 415 | a body in a media type the operation does not read |
//! | 422 | a JSON body that is not the schema's shape |
//! | 500 | a database that refuses everything but authentication |
//!
//! Each request is otherwise valid, built from the document's own schemas
//! by [`Examples`], so the status is the one being exercised and not an
//! earlier refusal. The domain statuses (2xx, 404, 409, upstream 502/503)
//! are each area's own tests.

use kynos::http::{Method as HttpMethod, StatusCode};
use kynos::test::{TestClient, TestRequest};
use serde_json::Value;
use stacks_core::entity::enums::Role;

use super::db::Credential;
use super::example::Examples;
use super::{Credentials, Env, TestUser};
use api::app::App;

/// An operation as the sweep reads it.
struct Operation {
    method: String,
    template: String,
    responses: Vec<String>,
    path_params: Vec<(String, Value)>,
    required_query: Vec<(String, Value)>,
    has_limit: bool,
    json_body: Option<Value>,
    multipart: bool,
    session: bool,
    admin: bool,
    /// The scope an API token needs for this operation, when tokens may call
    /// it.
    token_scope: Option<String>,
}

impl Operation {
    fn http_method(&self) -> HttpMethod {
        HttpMethod::from_bytes(self.method.to_ascii_uppercase().as_bytes()).expect("a method")
    }

    fn unsafe_method(&self) -> bool {
        !matches!(self.method.as_str(), "get" | "head" | "options")
    }

    fn path(&self, invalid: bool) -> String {
        let mut path = self.template.clone();
        for (name, schema) in &self.path_params {
            let value = if invalid {
                "!invalid".to_owned()
            } else {
                match schema {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                }
            };
            path = path.replace(&format!("{{{name}}}"), &value);
        }
        path
    }

    fn query(&self, extra: &[(&str, &str)], omit_required: bool) -> String {
        let mut pairs: Vec<(String, String)> = Vec::new();
        if !omit_required {
            for (name, value) in &self.required_query {
                let v = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                pairs.push((name.clone(), v));
            }
        }
        for (k, v) in extra {
            pairs.push(((*k).to_owned(), (*v).to_owned()));
        }
        if pairs.is_empty() {
            return String::new();
        }
        let encoded: String = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        format!("?{encoded}")
    }
}

fn operations(env: &Env, tag: &str) -> (Vec<Operation>, Value) {
    let document = serde_json::to_value(env.area(tag)).expect("the document as JSON");
    let full = serde_json::to_value(&env.document).expect("the document as JSON");
    let examples = Examples::new(&full);
    let mut out = Vec::new();
    let paths = document["paths"].as_object().expect("paths").clone();
    for (template, item) in paths {
        for (method, op) in item.as_object().expect("a path item") {
            if !matches!(method.as_str(), "get" | "put" | "post" | "delete" | "patch") {
                continue;
            }
            let params = op["parameters"].as_array().cloned().unwrap_or_default();
            let mut path_params = Vec::new();
            let mut required_query = Vec::new();
            let mut has_limit = false;
            for p in params {
                let name = p["name"].as_str().expect("a parameter name").to_owned();
                match p["in"].as_str() {
                    Some("path") => path_params.push((name, examples.of(&p["schema"]))),
                    Some("query") => {
                        has_limit |= name == "limit";
                        if p["required"] == Value::Bool(true) {
                            required_query.push((name, examples.of(&p["schema"])));
                        }
                    }
                    _ => {}
                }
            }
            let content = &op["requestBody"]["content"];
            let json_body = content
                .get("application/json")
                .map(|c| examples.of(&c["schema"]));
            let multipart = content.get("multipart/form-data").is_some();
            let security = op["security"].as_array().cloned().unwrap_or_default();
            let session = security.iter().any(|r| r.get("SessionCookie").is_some());
            let admin = security.iter().any(|r| {
                r["SessionCookie"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|v| v == "admin"))
            });
            let token_scope = security.iter().find_map(|r| {
                r["ApiToken"]
                    .as_array()
                    .and_then(|s| s.first())
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
            let responses = op["responses"]
                .as_object()
                .map(|r| r.keys().cloned().collect())
                .unwrap_or_default();
            out.push(Operation {
                method: method.clone(),
                template: template.clone(),
                responses,
                path_params,
                required_query,
                has_limit,
                json_body,
                multipart,
                session,
                admin,
                token_scope,
            });
        }
    }
    (out, full)
}

/// The refused accounts a sweep sends requests as. Each operation gets a
/// fresh caller of its own, since some (signing out, revoking) spend the
/// credential they are called with.
struct Cast {
    deactivated: TestUser,
    deactivated_admin: TestUser,
}

impl Cast {
    async fn new(env: &Env) -> Self {
        let deactivated = env.user(Role::User).await;
        env.deactivate(&deactivated).await;
        let deactivated_admin = env.user(Role::Admin).await;
        env.deactivate(&deactivated_admin).await;
        Self {
            deactivated,
            deactivated_admin,
        }
    }

    async fn caller(env: &Env, op: &Operation) -> TestUser {
        env.user(if op.admin { Role::Admin } else { Role::User })
            .await
    }

    fn refused(&self, op: &Operation) -> &TestUser {
        if op.admin {
            &self.deactivated_admin
        } else {
            &self.deactivated
        }
    }
}

/// `request`, carrying `who`'s session when `op` takes one.
fn as_caller<'c>(
    request: TestRequest<'c, App>,
    op: &Operation,
    who: &TestUser,
) -> TestRequest<'c, App> {
    if op.session {
        request.session(who)
    } else {
        request
    }
}

/// A request for `op` that passes every extractor, as `who` when given.
fn valid<'c>(
    client: &'c TestClient<App>,
    op: &Operation,
    who: Option<&TestUser>,
) -> TestRequest<'c, App> {
    let mut request = client.method(
        op.http_method(),
        &format!("{}{}", op.path(false), op.query(&[], false)),
    );
    if let (true, Some(who)) = (op.session, who) {
        request = request.session(who);
    }
    if let Some(body) = &op.json_body {
        request = request.json(body);
    }
    request
}

/// Drive every shared status the operations tagged `tag` declare, and
/// check every response the sweep saw against the description.
pub async fn sweep(env: &Env, tag: &str) {
    let (ops, _) = operations(env, tag);
    let cast = Cast::new(env).await;
    let client = env.client();
    let limited = env.limited_client();
    let degraded_session = env.degraded_client(Credential::Session).await;
    let degraded_token = env.degraded_client(Credential::Token).await;
    // The limited client's one request a minute, spent up front.
    limited.get("/api/v1/me").send().await;

    for op in &ops {
        let declares = |status: &str| op.responses.iter().any(|s| s == status);
        let caller = Cast::caller(env, op).await;
        let who = &caller;

        if declares("401") {
            valid(&client, op, None)
                .send()
                .await
                .assert_status(StatusCode::UNAUTHORIZED);
        }
        if declares("403") {
            let response = if op.session {
                valid(&client, op, Some(cast.refused(op))).send().await
            } else {
                valid(&client, op, None)
                    .header("sec-fetch-site", "cross-site")
                    .send()
                    .await
            };
            response.assert_status(StatusCode::FORBIDDEN);
        }
        if declares("406") {
            valid(&client, op, Some(who))
                .header("accept-encoding", "identity;q=0, *;q=0")
                .send()
                .await
                .assert_status(StatusCode::NOT_ACCEPTABLE);
        }
        if declares("429") {
            valid(&limited, op, Some(who))
                .send()
                .await
                .assert_status(StatusCode::TOO_MANY_REQUESTS);
        }
        if declares("400") {
            let request = if !op.path_params.is_empty() {
                let mut r = client.method(
                    op.http_method(),
                    &format!("{}{}", op.path(true), op.query(&[], false)),
                );
                if op.session {
                    r = r.session(who);
                }
                if let Some(body) = &op.json_body {
                    r = r.json(body);
                }
                Some(r)
            } else if op.has_limit {
                let mut r = client.method(
                    op.http_method(),
                    &format!("{}{}", op.path(false), op.query(&[("limit", "0")], false)),
                );
                if op.session {
                    r = r.session(who);
                }
                Some(r)
            } else if op.json_body.is_some() {
                let mut r = client
                    .method(
                        op.http_method(),
                        &format!("{}{}", op.path(false), op.query(&[], false)),
                    )
                    .body("application/json", "{");
                if op.session {
                    r = r.session(who);
                }
                Some(r)
            } else if !op.required_query.is_empty() {
                let mut r = client.method(
                    op.http_method(),
                    &format!("{}{}", op.path(false), op.query(&[], true)),
                );
                if op.session {
                    r = r.session(who);
                }
                Some(r)
            } else {
                None
            };
            if let Some(request) = request {
                request.send().await.assert_status(StatusCode::BAD_REQUEST);
            }
        }
        if op.json_body.is_some() {
            let target = format!("{}{}", op.path(false), op.query(&[], false));
            let with = |r| as_caller(r, op, who);
            if declares("413") {
                with(
                    client
                        .method(op.http_method(), &target)
                        .body("application/json", "{}"),
                )
                .content_length(3 * 1024 * 1024)
                .send()
                .await
                .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
            }
            if declares("415") {
                with(
                    client
                        .method(op.http_method(), &target)
                        .body("text/plain", "x"),
                )
                .send()
                .await
                .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
            }
            if declares("422") {
                with(
                    client
                        .method(op.http_method(), &target)
                        .body("application/json", "[]"),
                )
                .send()
                .await
                .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
        if declares("500") && !op.multipart {
            // Through a session, then through a token, so an operation that
            // reads only the other credential's table still fails.
            let failed = valid(&degraded_session, op, Some(who)).send().await;
            if failed.status() != StatusCode::INTERNAL_SERVER_ERROR
                && let Some(scope) = &op.token_scope
            {
                let token = match scope.as_str() {
                    "admin" => who.admin.clone().expect("an administrator's token"),
                    "write" => who.write.clone(),
                    _ => who.read.clone(),
                };
                let mut request = degraded_token.method(
                    op.http_method(),
                    &format!("{}{}", op.path(false), op.query(&[], false)),
                );
                request = request.bearer(&token);
                if let Some(body) = &op.json_body {
                    request = request.json(body);
                }
                request.send().await;
            }
        }
        // Last, since it may spend the caller's credential: whatever a valid
        // request yields (often a 404 for an invented id) counts too.
        valid(&client, op, Some(who)).send().await;
    }
    client.assert_conformance();
    limited.assert_conformance();
    degraded_session.assert_conformance();
    degraded_token.assert_conformance();
}

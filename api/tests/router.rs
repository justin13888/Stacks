//! Router / OpenAPI contract tests.
//!
//! These are pure functions of the router, so they need no database and run in
//! CI without a Postgres service.

use std::collections::BTreeSet;

fn spec() -> serde_json::Value {
    api::app::openapi_json()
}

fn paths(spec: &serde_json::Value) -> BTreeSet<String> {
    spec["paths"]
        .as_object()
        .expect("spec has paths")
        .keys()
        .cloned()
        .collect()
}

/// Salvo 0.77 parses path parameters as `{name}`; `<` and `>` are name
/// terminators. A route registered as `media/<cameo_id>` therefore matches only
/// the literal segment `<cameo_id>` and 404s for every real id — while still
/// appearing correctly in the generated OpenAPI document, which is why this went
/// unnoticed. Nothing but a test can distinguish the two from the spec alone, so
/// assert the angle form never comes back.
#[test]
fn no_route_uses_angle_bracket_parameters() {
    let spec = spec();
    let offenders: Vec<_> = paths(&spec)
        .into_iter()
        .filter(|p| p.contains('<') || p.contains('>'))
        .collect();
    assert!(
        offenders.is_empty(),
        "these routes use Salvo's non-parameter angle syntax and cannot match: {offenders:?}"
    );
}

/// Every `{param}` in a path must be declared as a path parameter, or the
/// generated client emits a call it cannot fill in.
#[test]
fn every_path_parameter_is_declared() {
    let spec = spec();
    let mut missing = Vec::new();

    for (path, ops) in spec["paths"].as_object().unwrap() {
        let expected: BTreeSet<String> = path
            .split('/')
            .filter_map(|seg| {
                seg.strip_prefix('{')
                    .and_then(|s| s.strip_suffix('}'))
                    .map(str::to_string)
            })
            .collect();
        if expected.is_empty() {
            continue;
        }
        for (verb, op) in ops.as_object().unwrap() {
            if !matches!(verb.as_str(), "get" | "post" | "put" | "patch" | "delete") {
                continue;
            }
            let declared: BTreeSet<String> = op["parameters"]
                .as_array()
                .map(|ps| {
                    ps.iter()
                        .filter(|p| p["in"] == "path")
                        .filter_map(|p| p["name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let gap: Vec<_> = expected.difference(&declared).cloned().collect();
            if !gap.is_empty() {
                missing.push(format!("{verb} {path}: {gap:?}"));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "operations with undeclared path parameters: {missing:#?}"
    );
}

/// The auth flow's shape is load-bearing and was previously wrong in three
/// places at once, so pin it explicitly.
#[test]
fn auth_routes_have_the_expected_methods() {
    let spec = spec();
    let p = &spec["paths"];

    assert!(
        p["/api/v1/auth/login"].get("post").is_some(),
        "login must be POST — the web client calls it with POST"
    );
    assert!(
        p["/api/v1/auth/callback"].get("post").is_some(),
        "callback must be POST with a JSON body, not GET with query params"
    );
    assert!(
        p["/api/v1/auth/callback"].get("get").is_none(),
        "the GET callback was replaced by the POST form"
    );
    assert!(p["/api/v1/auth/refresh"].get("post").is_some());
}

/// The Library page depends on this endpoint; it is the only way to list a
/// user's tracked media.
#[test]
fn library_endpoint_is_exposed() {
    let spec = spec();
    assert!(spec["paths"]["/api/v1/users/me/library"]["get"].is_object());
}

/// Admin bootstrap is useless without a way to change roles afterwards.
#[test]
fn admin_role_endpoint_is_exposed() {
    let spec = spec();
    assert!(spec["paths"]["/api/v1/users/{id}/role"]["patch"].is_object());
}

/// cameo exposes no upcoming feed, so the route was removed rather than left
/// returning a permanently empty list.
#[test]
fn upcoming_discover_route_is_gone() {
    assert!(!paths(&spec()).contains("/api/v1/discover/upcoming"));
}

/// A rough guard against a whole router being dropped from `build_router_stub`
/// — the spec is generated from the stub, not the real router, so the two can
/// silently drift.
#[test]
fn every_area_router_appears_in_the_spec() {
    let paths = paths(&spec());
    for expected in [
        "/api/v1/users/me",
        "/api/v1/users/me/library",
        "/api/v1/users/me/lists",
        "/api/v1/users/me/tags",
        "/api/v1/users/me/blocklist",
        "/api/v1/users/me/stats",
        "/api/v1/users/me/export",
        "/api/v1/users/me/import",
        "/api/v1/users/me/history",
        "/api/v1/users/me/notification-channels",
        "/api/v1/media/search",
        "/api/v1/discover/trending",
        "/api/v1/admin/servers",
        "/api/v1/admin/jobs",
        "/health",
        "/metrics",
    ] {
        assert!(paths.contains(expected), "missing route: {expected}");
    }
}

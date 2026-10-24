//! The baseline schema's own guarantees, tested against PostgreSQL 18:
//! up/down, CHECK constraints, no cascades, append-only plays and events,
//! and undeletable user data.

mod common;

use migration::MigratorTrait;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, Iterable, Statement, TransactionTrait,
};
use stacks_core::entity::enums::*;

const U: &str = "00000000-0000-7000-8000-000000000001";
const M: &str = "00000000-0000-7000-8000-000000000002";
const S: &str = "00000000-0000-7000-8000-000000000003";
const E: &str = "00000000-0000-7000-8000-000000000004";
const I: &str = "00000000-0000-7000-8000-000000000005";
const LE: &str = "00000000-0000-7000-8000-000000000006";
const R: &str = "00000000-0000-7000-8000-000000000007";
const L: &str = "00000000-0000-7000-8000-000000000008";
const T: &str = "00000000-0000-7000-8000-000000000009";
const C: &str = "00000000-0000-7000-8000-00000000000a";
const P: &str = "00000000-0000-7000-8000-00000000000b";
const EV: &str = "00000000-0000-7000-8000-00000000000c";

/// One valid row in every table.
async fn seed(db: &DatabaseConnection) {
    let sql = format!(
        r#"
        INSERT INTO users (id, oidc_issuer, oidc_subject, display_name) VALUES ('{U}', 'iss', 'sub', 'u');
        INSERT INTO sessions (user_id, token_hash, expires_at) VALUES ('{U}', repeat('a', 64), now() + interval '1 day');
        INSERT INTO api_tokens (user_id, name, token_prefix, lookup_hash, scopes)
            VALUES ('{U}', 't', 'stk_abcd', repeat('b', 64), ARRAY['read']);
        INSERT INTO oidc_login_states (state, nonce, pkce_verifier, expires_at) VALUES ('st', 'n', 'v', now());
        INSERT INTO media (id, kind, category, title) VALUES ('{M}', 'show', 'series', 'T');
        INSERT INTO media_refs (media_id, provider, kind, external_id, is_primary) VALUES ('{M}', 'tmdb', 'show', '1', true);
        INSERT INTO seasons (id, media_id, number) VALUES ('{S}', '{M}', 1);
        INSERT INTO episodes (id, media_id, season_id, number) VALUES ('{E}', '{M}', '{S}', 1);
        INSERT INTO imports (id, user_id, source, original_size) VALUES ('{I}', '{U}', 'mal_xml', 0);
        INSERT INTO import_items (import_id, row_number, raw) VALUES ('{I}', 1, '{{}}');
        INSERT INTO library_entries (id, user_id, media_id, status) VALUES ('{LE}', '{U}', '{M}', 'planning');
        INSERT INTO watch_runs (id, entry_id, run_number) VALUES ('{R}', '{LE}', 1);
        INSERT INTO plays (id, run_id, episode_id, source) VALUES ('{P}', '{R}', '{E}', 'manual');
        INSERT INTO entry_events (id, entry_id, user_id, kind) VALUES ('{EV}', '{LE}', '{U}', 'entry_created');
        INSERT INTO lists (id, user_id, name) VALUES ('{L}', '{U}', 'l');
        INSERT INTO list_items (list_id, media_id, position) VALUES ('{L}', '{M}', 'V');
        INSERT INTO tags (id, user_id, name) VALUES ('{T}', '{U}', 'x');
        INSERT INTO entry_tags (entry_id, tag_id) VALUES ('{LE}', '{T}');
        INSERT INTO blocklist (user_id, media_id) VALUES ('{U}', '{M}');
        INSERT INTO jobs (job_type) VALUES ('import');
        INSERT INTO notification_channels (id, user_id, channel_type, name) VALUES ('{C}', '{U}', 'webhook', 'n');
        INSERT INTO notification_subscriptions (user_id, channel_id, event_type) VALUES ('{U}', '{C}', 'new_episode');
        "#
    );
    db.execute_unprepared(&sql).await.expect("seed every table");
}

/// Run `sql` in a transaction that is always rolled back; return its error.
async fn try_sql(db: &DatabaseConnection, sql: &str) -> Result<(), String> {
    let txn = db.begin().await.unwrap();
    let res = txn.execute_unprepared(sql).await.map(|_| ());
    txn.rollback().await.unwrap();
    res.map_err(|e| e.to_string())
}

async fn assert_rejected_by_check(db: &DatabaseConnection, sql: &str) {
    let err = try_sql(db, sql)
        .await
        .expect_err(&format!("the database must reject: {sql}"));
    assert!(
        err.contains("check constraint"),
        "expected a CHECK violation for `{sql}`, got: {err}"
    );
}

/// For each enum-backed column: every Rust variant is accepted by the
/// database, and an invalid value is rejected by the database itself.
#[tokio::test]
async fn every_enum_column_has_a_check_constraint() {
    let t = test_db!();
    let db = &t.db;
    seed(db).await;

    // (values, statement template with `{v}`)
    let cases: Vec<(Vec<&'static str>, String)> = vec![
        (
            Role::iter().map(Role::as_str).collect(),
            "UPDATE users SET role = '{v}'".into(),
        ),
        (
            MediaKind::iter().map(MediaKind::as_str).collect(),
            "UPDATE media SET kind = '{v}'".into(),
        ),
        (
            MediaFormat::iter().map(MediaFormat::as_str).collect(),
            "UPDATE media SET format = '{v}'".into(),
        ),
        (
            Category::iter().map(Category::as_str).collect(),
            "UPDATE media SET category = '{v}'".into(),
        ),
        (
            ReleaseStatus::iter().map(ReleaseStatus::as_str).collect(),
            "UPDATE media SET status = '{v}'".into(),
        ),
        (
            RefProvider::iter().map(RefProvider::as_str).collect(),
            "UPDATE media_refs SET provider = '{v}'".into(),
        ),
        (
            MediaKind::iter().map(MediaKind::as_str).collect(),
            "UPDATE media_refs SET kind = '{v}'".into(),
        ),
        (
            EntryStatus::iter().map(EntryStatus::as_str).collect(),
            "UPDATE library_entries SET status = '{v}'".into(),
        ),
        (
            DatePrecision::iter().map(DatePrecision::as_str).collect(),
            "UPDATE watch_runs SET started_on = '2020-01-01', started_precision = '{v}'".into(),
        ),
        (
            DatePrecision::iter().map(DatePrecision::as_str).collect(),
            "UPDATE watch_runs SET completed_on = '2020-01-01', completed_precision = '{v}'".into(),
        ),
        (
            PlaySource::iter().map(PlaySource::as_str).collect(),
            format!("INSERT INTO plays (run_id, source) VALUES ('{R}', '{{v}}')"),
        ),
        (
            EntryEventKind::iter().map(EntryEventKind::as_str).collect(),
            format!(
                "INSERT INTO entry_events (entry_id, user_id, kind) VALUES ('{LE}', '{U}', '{{v}}')"
            ),
        ),
        (
            ImportSource::iter().map(ImportSource::as_str).collect(),
            "UPDATE imports SET source = '{v}'".into(),
        ),
        (
            ImportStatus::iter().map(ImportStatus::as_str).collect(),
            "UPDATE imports SET status = '{v}'".into(),
        ),
        (
            ConflictStrategy::iter()
                .map(ConflictStrategy::as_str)
                .collect(),
            "UPDATE imports SET conflict_strategy = '{v}'".into(),
        ),
        (
            ImportOutcome::iter().map(ImportOutcome::as_str).collect(),
            "UPDATE import_items SET outcome = '{v}'".into(),
        ),
        (
            JobType::iter().map(JobType::as_str).collect(),
            "UPDATE jobs SET job_type = '{v}'".into(),
        ),
        (
            JobStatus::iter().map(JobStatus::as_str).collect(),
            "UPDATE jobs SET status = '{v}'".into(),
        ),
        (
            ChannelType::iter().map(ChannelType::as_str).collect(),
            "UPDATE notification_channels SET channel_type = '{v}'".into(),
        ),
        (
            NotificationEventType::iter()
                .map(NotificationEventType::as_str)
                .collect(),
            "UPDATE notification_subscriptions SET event_type = '{v}'".into(),
        ),
        (
            Scope::iter().map(Scope::as_str).collect(),
            "UPDATE api_tokens SET scopes = ARRAY['{v}']".into(),
        ),
    ];

    for (values, template) in &cases {
        for v in values {
            let sql = template.replace("{v}", v);
            try_sql(db, &sql)
                .await
                .unwrap_or_else(|e| panic!("valid value rejected: {sql}: {e}"));
        }
        assert_rejected_by_check(db, &template.replace("{v}", "not-a-valid-value")).await;
    }

    // Constraints that are not a single enum but still guard user data.
    for sql in [
        "UPDATE library_entries SET score = 101",
        "UPDATE library_entries SET score = -1",
        "UPDATE watch_runs SET started_on = '2020-01-01'", // date without precision
        "UPDATE users SET settings = '{\"theme\": \"neon\"}'",
        "UPDATE users SET settings = '{\"score_format\": \"stars\"}'",
        "UPDATE users SET settings = '{\"title_language\": \"klingon\"}'",
        "UPDATE users SET settings = '{\"spoilers\": \"maybe\"}'",
        "UPDATE users SET settings = '[]'",
        "UPDATE api_tokens SET scopes = '{}'",
        "UPDATE list_items SET position = 'V0'",
        "UPDATE list_items SET position = ''",
        "UPDATE media SET refresh_failures = -1",
        "UPDATE users SET display_name = '  '",
    ] {
        assert_rejected_by_check(db, sql).await;
    }
}

/// No foreign key anywhere cascades a delete, and every one restricts.
#[tokio::test]
async fn no_foreign_key_cascades() {
    let t = test_db!();
    let rows =
        t.db.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT conrelid::regclass::text AS tbl, conname, confdeltype::text AS del \
             FROM pg_constraint WHERE contype = 'f'",
        ))
        .await
        .unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        let tbl: String = row.try_get("", "tbl").unwrap();
        let name: String = row.try_get("", "conname").unwrap();
        let del: String = row.try_get("", "del").unwrap();
        // 'r' = RESTRICT
        assert_eq!(del, "r", "{tbl}.{name} must be ON DELETE RESTRICT");
    }
}

/// Every table's primary key is a single `uuid` column.
#[tokio::test]
async fn every_primary_key_is_a_uuid() {
    let t = test_db!();
    let rows =
        t.db.query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT c.relname AS tbl, \
                    array_length(i.indkey, 1) AS cols, \
                    format_type(a.atttypid, a.atttypmod) AS typ \
             FROM pg_index i \
             JOIN pg_class c ON c.oid = i.indrelid \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = i.indkey[0] \
             WHERE i.indisprimary AND n.nspname = 'public' AND c.relname <> 'seaql_migrations'",
        ))
        .await
        .unwrap();
    assert!(rows.len() >= 20, "expected every table, got {}", rows.len());
    for row in rows {
        let tbl: String = row.try_get("", "tbl").unwrap();
        let cols: i32 = row.try_get("", "cols").unwrap();
        let typ: String = row.try_get("", "typ").unwrap();
        assert_eq!((cols, typ.as_str()), (1, "uuid"), "{tbl}");
    }
}

/// Plays may only be voided and restored; events never change; neither (nor
/// users, entries, runs, lists) can be deleted.
#[tokio::test]
async fn append_only_and_undeletable_tables() {
    let t = test_db!();
    let db = &t.db;
    seed(db).await;

    for sql in [
        "DELETE FROM plays".to_string(),
        "DELETE FROM entry_events".to_string(),
        "DELETE FROM users".to_string(),
        "DELETE FROM library_entries".to_string(),
        "DELETE FROM watch_runs".to_string(),
        "DELETE FROM lists".to_string(),
        "UPDATE entry_events SET kind = 'entry_updated'".to_string(),
        "UPDATE entry_events SET after = '{}'".to_string(),
        "UPDATE plays SET watched_at = now()".to_string(),
        "UPDATE plays SET episode_id = NULL".to_string(),
        format!("UPDATE plays SET run_id = run_id, source = 'bulk' WHERE id = '{P}'"),
    ] {
        let err = try_sql(db, &sql)
            .await
            .expect_err(&format!("must be refused: {sql}"));
        assert!(
            err.contains("cannot be deleted")
                || err.contains("immutable")
                || err.contains("append-only"),
            "`{sql}` refused for the wrong reason: {err}"
        );
    }

    // A play can be voided, and restored, but a void's time is kept.
    try_sql(db, "UPDATE plays SET voided_at = now()")
        .await
        .expect("a play can be voided");
    db.execute_unprepared("UPDATE plays SET voided_at = '2026-01-01T00:00:00Z'")
        .await
        .unwrap();
    let err = try_sql(db, "UPDATE plays SET voided_at = '2026-02-01T00:00:00Z'")
        .await
        .expect_err("rewriting a void's time is refused");
    assert!(err.contains("never rewritten"), "{err}");
    try_sql(db, "UPDATE plays SET voided_at = voided_at")
        .await
        .expect("an unchanged void is no change");
    db.execute_unprepared("UPDATE plays SET voided_at = NULL")
        .await
        .expect("a voided play can be restored");
    try_sql(db, "UPDATE plays SET voided_at = now()")
        .await
        .expect("a restored play can be voided again");
}

/// `updated_at` is maintained by the database on every update.
#[tokio::test]
async fn updated_at_is_maintained_by_trigger() {
    let t = test_db!();
    let db = &t.db;
    seed(db).await;
    db.execute_unprepared(&format!(
        "UPDATE library_entries SET updated_at = '2000-01-01' WHERE id = '{LE}'; \
         UPDATE library_entries SET notes = 'n' WHERE id = '{LE}';"
    ))
    .await
    .unwrap();
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            format!("SELECT updated_at > now() - interval '1 minute' AS fresh FROM library_entries WHERE id = '{LE}'"),
        ))
        .await
        .unwrap()
        .unwrap();
    assert!(row.try_get::<bool>("", "fresh").unwrap());
}

/// The baseline applies to an empty database (the harness does that) and
/// `down` removes every object it created; it then re-applies cleanly.
#[tokio::test]
async fn migration_down_drops_everything_and_reapplies() {
    let t = test_db!();
    let db = &t.db;
    migration::Migrator::down(db, None).await.expect("down");
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT \
               (SELECT count(*) FROM information_schema.tables \
                  WHERE table_schema = 'public' AND table_name <> 'seaql_migrations') AS tables, \
               (SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                  WHERE n.nspname = 'public' AND p.proname LIKE 'stacks_%') AS functions",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.try_get::<i64>("", "tables").unwrap(),
        0,
        "tables left behind"
    );
    assert_eq!(
        row.try_get::<i64>("", "functions").unwrap(),
        0,
        "functions left behind"
    );
    migration::Migrator::up(db, None).await.expect("re-apply");
    seed(db).await;
}

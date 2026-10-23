//! The one destructive path: permanently removing a user and everything they
//! own.
//!
//! The schema refuses to hard-delete users, entries, runs, plays, events and
//! lists, and no foreign key cascades. Purge is the sanctioned exception: it
//! opts out of the delete guards for its own transaction only
//! (`SET LOCAL stacks.allow_purge`) and removes rows child-first. It demands
//! two confirmations: the account must already be deactivated, and the caller
//! must repeat the account's id and display name.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait};
use uuid::Uuid;

use super::{UserError, repo};

/// What the caller must restate to purge an account.
#[derive(Debug, Clone)]
pub struct PurgeConfirmation {
    pub user_id: Uuid,
    pub display_name: String,
}

/// Child-first deletes; each binds the user id as `$1`.
const PURGE_STATEMENTS: &[&str] = &[
    "DELETE FROM entry_tags WHERE entry_id IN (SELECT id FROM library_entries WHERE user_id = $1)",
    "DELETE FROM plays WHERE run_id IN (SELECT r.id FROM watch_runs r \
     JOIN library_entries e ON e.id = r.entry_id WHERE e.user_id = $1)",
    "DELETE FROM entry_events WHERE user_id = $1",
    "DELETE FROM watch_runs WHERE entry_id IN (SELECT id FROM library_entries WHERE user_id = $1)",
    "DELETE FROM list_items WHERE list_id IN (SELECT id FROM lists WHERE user_id = $1)",
    "DELETE FROM lists WHERE user_id = $1",
    "DELETE FROM blocklist WHERE user_id = $1",
    "DELETE FROM tags WHERE user_id = $1",
    "DELETE FROM library_entries WHERE user_id = $1",
    "DELETE FROM notification_subscriptions WHERE user_id = $1",
    "DELETE FROM notification_channels WHERE user_id = $1",
    "DELETE FROM import_items WHERE import_id IN (SELECT id FROM imports WHERE user_id = $1)",
    "DELETE FROM imports WHERE user_id = $1",
    "DELETE FROM api_tokens WHERE user_id = $1",
    "DELETE FROM sessions WHERE user_id = $1",
    "DELETE FROM users WHERE id = $1",
];

/// Permanently remove a deactivated user and all of their data.
pub async fn purge(db: &DatabaseConnection, confirm: &PurgeConfirmation) -> Result<(), UserError> {
    let user = repo::find_by_id(db, confirm.user_id)
        .await?
        .ok_or(UserError::NotFound)?;
    if user.deactivated_at.is_none() {
        return Err(UserError::PurgeRequiresDeactivation);
    }
    if user.display_name != confirm.display_name {
        return Err(UserError::PurgeNotConfirmed);
    }
    let txn = db.begin().await?;
    txn.execute_unprepared("SET LOCAL stacks.allow_purge = 'on'")
        .await?;
    for sql in PURGE_STATEMENTS {
        txn.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            *sql,
            [user.id.into()],
        ))
        .await?;
    }
    txn.commit().await?;
    tracing::warn!(user_id = %user.id, "user purged");
    Ok(())
}

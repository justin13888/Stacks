//! The one destructive path: permanently removing a user and everything they
//! own.
//!
//! The schema refuses to hard-delete users, entries, runs, plays, events and
//! lists, and no foreign key cascades. Purge is the sanctioned exception: it
//! opts out of the delete guards for its own transaction only
//! (`SET LOCAL stacks.allow_purge`) and removes rows child-first. It demands
//! two confirmations: the account must already be deactivated, and the caller
//! must repeat the account's id. An administrator cannot purge themselves.

use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QuerySelect, Statement,
    TransactionTrait,
};
use uuid::Uuid;

use super::UserError;
use crate::entity::user;

/// What the caller must restate to purge an account.
#[derive(Debug, Clone, Copy)]
pub struct PurgeConfirmation {
    /// The account to purge.
    pub user_id: Uuid,
    /// The account's id, repeated: anything else refuses the purge.
    pub confirmed_id: Uuid,
}

/// Child-first deletes; each binds the user id as `$1`. Every table holding
/// a `user_id`, or a row that hangs off one, has its line, and so do the
/// queued jobs whose payload names the user or one of their imports.
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
    "DELETE FROM jobs WHERE payload->>'user_id' = $1::text \
     OR payload->>'import_id' IN (SELECT id::text FROM imports WHERE user_id = $1)",
    "DELETE FROM import_items WHERE import_id IN (SELECT id FROM imports WHERE user_id = $1)",
    "DELETE FROM imports WHERE user_id = $1",
    "DELETE FROM api_tokens WHERE user_id = $1",
    "DELETE FROM sessions WHERE user_id = $1",
    "DELETE FROM users WHERE id = $1",
];

/// Permanently remove a deactivated user and all of their data, on
/// `actor_id`'s authority, in one transaction.
///
/// The account's row is locked first, so it cannot be reactivated between
/// the checks and the deletes. The audit line names the two accounts by id
/// only.
pub async fn purge(
    db: &DatabaseConnection,
    actor_id: Uuid,
    confirm: PurgeConfirmation,
) -> Result<(), UserError> {
    let txn = db.begin().await?;
    let user = user::Entity::find_by_id(confirm.user_id)
        .lock_exclusive()
        .one(&txn)
        .await?
        .ok_or(UserError::NotFound)?;
    if user.id == actor_id {
        return Err(UserError::CannotPurgeOwnAccount);
    }
    if user.deactivated_at.is_none() {
        return Err(UserError::PurgeRequiresDeactivation);
    }
    if confirm.confirmed_id != user.id {
        return Err(UserError::PurgeNotConfirmed);
    }
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
    tracing::warn!(actor_id = %actor_id, user_id = %user.id, "user purged");
    Ok(())
}

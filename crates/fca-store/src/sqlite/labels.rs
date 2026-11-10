//! SQL for labels. Labels are created implicitly the first time they are used.
//!
//! Must NOT decide which labels a transaction should have; callers pass names in.

use sqlx::SqliteConnection;

use crate::{LabelCount, StoreError};

/// Decode the `group_concat(name, char(31))` column used by list queries, sorted.
pub(super) fn split_concat(concat: Option<String>) -> Vec<String> {
    let mut names: Vec<String> = concat
        .map(|s| s.split('\u{1f}').map(str::to_string).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Create the label if it does not exist yet.
async fn ensure(db: &mut SqliteConnection, name: &str) -> Result<(), StoreError> {
    sqlx::query("INSERT OR IGNORE INTO labels (name) VALUES (?)")
        .bind(name)
        .execute(&mut *db)
        .await?;
    Ok(())
}

/// Give recurring entry `recurring_id` all of `names`, creating labels as needed.
pub(super) async fn attach_to_recurring(
    db: &mut SqliteConnection,
    recurring_id: i64,
    names: &[String],
) -> Result<(), StoreError> {
    for name in names {
        ensure(db, name).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO recurring_labels (recurring_id, label_id) \
             SELECT ?, id FROM labels WHERE name = ?",
        )
        .bind(recurring_id)
        .bind(name)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}

/// Give transaction `transaction_id` all of `names`, creating labels as needed.
pub(super) async fn attach(
    db: &mut SqliteConnection,
    transaction_id: i64,
    names: &[String],
) -> Result<(), StoreError> {
    for name in names {
        ensure(db, name).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO transaction_labels (transaction_id, label_id) \
             SELECT ?, id FROM labels WHERE name = ?",
        )
        .bind(transaction_id)
        .bind(name)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}

/// Remove `names` from a transaction. Unknown or unattached names are ignored.
pub(super) async fn detach(
    db: &mut SqliteConnection,
    transaction_id: i64,
    names: &[String],
) -> Result<(), StoreError> {
    for name in names {
        sqlx::query(
            "DELETE FROM transaction_labels WHERE transaction_id = ? \
             AND label_id = (SELECT id FROM labels WHERE name = ?)",
        )
        .bind(transaction_id)
        .bind(name)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}

pub(super) async fn list_with_counts(
    db: &mut SqliteConnection,
) -> Result<Vec<LabelCount>, StoreError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT l.name, COUNT(*) FROM labels l \
         JOIN transaction_labels tl ON tl.label_id = l.id \
         GROUP BY l.id ORDER BY l.name",
    )
    .fetch_all(&mut *db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name, count)| LabelCount { name, count })
        .collect())
}

//! SQL for transactions: insert, list with a `Filter`, modify, delete.
//!
//! Functions take a connection so the caller decides the DB transaction scope.
//! Must NOT know about recurring schedules or CSV formats.

use chrono::NaiveDate;
use fca_core::{Filter, Kind, Money, NewTransaction, Origin, Transaction, TransactionPatch};
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use super::labels;
use crate::StoreError;

/// Columns of a `Transaction`, with its labels folded into one text column.
const SELECT: &str = "SELECT t.id, t.date, t.amount, t.currency, t.description, t.account, \
    t.payee, t.origin, t.recurring_id, t.transfer_id, \
    (SELECT group_concat(l.name, char(31)) FROM transaction_labels tl \
     JOIN labels l ON l.id = tl.label_id WHERE tl.transaction_id = t.id) AS labels \
    FROM transactions t";

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    date: NaiveDate,
    amount: i64,
    currency: String,
    description: String,
    account: String,
    payee: Option<String>,
    origin: String,
    recurring_id: Option<i64>,
    transfer_id: Option<i64>,
    /// Label names joined by the ASCII unit separator (char 31), or NULL.
    labels: Option<String>,
}

impl Row {
    fn into_transaction(self) -> Result<Transaction, StoreError> {
        let origin: Origin = self
            .origin
            .parse()
            .map_err(|e| StoreError::Corrupt(format!("{e}")))?;
        let labels = labels::split_concat(self.labels);
        Ok(Transaction {
            id: self.id,
            date: self.date,
            amount: Money::from_minor(self.amount),
            currency: self.currency,
            description: self.description,
            account: self.account,
            payee: self.payee,
            origin,
            recurring_id: self.recurring_id,
            transfer_id: self.transfer_id,
            labels,
        })
    }
}

/// Insert one row plus its labels. `None` if a uniqueness rule made it a duplicate.
pub(super) async fn insert_one(
    db: &mut SqliteConnection,
    tx: &NewTransaction,
) -> Result<Option<i64>, StoreError> {
    let result = sqlx::query(
        "INSERT INTO transactions \
         (date, amount, currency, description, account, payee, origin, recurring_id, import_hash) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(tx.date)
    .bind(tx.amount.minor())
    .bind(&tx.currency)
    .bind(&tx.description)
    .bind(&tx.account)
    .bind(&tx.payee)
    .bind(tx.origin.as_str())
    .bind(tx.recurring_id)
    .bind(&tx.import_hash)
    .execute(&mut *db)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    let id = result.last_insert_rowid();
    labels::attach(db, id, &tx.labels).await?;
    Ok(Some(id))
}

pub(super) async fn list(
    db: &mut SqliteConnection,
    filter: &Filter,
) -> Result<Vec<Transaction>, StoreError> {
    let mut q: QueryBuilder<Sqlite> = QueryBuilder::new(SELECT);
    q.push(" WHERE 1 = 1");
    for label in &filter.labels {
        q.push(
            " AND EXISTS (SELECT 1 FROM transaction_labels tl JOIN labels l ON l.id = tl.label_id \
             WHERE tl.transaction_id = t.id AND l.name = ",
        )
        .push_bind(label.clone())
        .push(")");
    }
    if let Some(account) = &filter.account {
        q.push(" AND t.account = ")
            .push_bind(account.clone())
            .push(" COLLATE NOCASE");
    }
    if let Some(payee) = &filter.payee {
        q.push(" AND t.payee = ")
            .push_bind(payee.clone())
            .push(" COLLATE NOCASE");
    }
    if let Some(from) = filter.from {
        q.push(" AND t.date >= ").push_bind(from);
    }
    if let Some(to) = filter.to {
        q.push(" AND t.date <= ").push_bind(to);
    }
    if let Some(origin) = filter.origin {
        q.push(" AND t.origin = ").push_bind(origin.as_str());
    }
    // Income and expense never include transfers between your own accounts.
    match filter.kind {
        Some(Kind::Income) => q.push(" AND t.amount > 0 AND t.transfer_id IS NULL"),
        Some(Kind::Expense) => q.push(" AND t.amount < 0 AND t.transfer_id IS NULL"),
        Some(Kind::Transfer) => q.push(" AND t.transfer_id IS NOT NULL"),
        None => &mut q,
    };
    q.push(" ORDER BY t.date, t.id");

    let rows: Vec<Row> = q.build_query_as().fetch_all(&mut *db).await?;
    rows.into_iter().map(Row::into_transaction).collect()
}

pub(super) async fn get(db: &mut SqliteConnection, id: i64) -> Result<Transaction, StoreError> {
    let row: Option<Row> = sqlx::query_as(&format!("{SELECT} WHERE t.id = ?"))
        .bind(id)
        .fetch_optional(&mut *db)
        .await?;
    row.ok_or(StoreError::NotFound {
        what: "transaction",
        id,
    })?
    .into_transaction()
}

/// A transfer's sides must stay in different accounts and keep summing to zero.
/// `t` already has the patch applied; the other side follows an amount change.
async fn keep_transfer_consistent(
    db: &mut SqliteConnection,
    t: &Transaction,
    transfer_id: i64,
    patch: &TransactionPatch,
) -> Result<(), StoreError> {
    if patch.account.is_some() {
        let clash: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM transactions WHERE transfer_id = ? AND id != ? AND account = ? COLLATE NOCASE",
        )
        .bind(transfer_id)
        .bind(t.id)
        .bind(&t.account)
        .fetch_one(&mut *db)
        .await?;
        if clash > 0 {
            return Err(StoreError::Invalid(format!(
                "the other side of this transfer is already in account `{}`",
                t.account
            )));
        }
    }
    if patch.amount.is_some() {
        sqlx::query("UPDATE transactions SET amount = ? WHERE transfer_id = ? AND id != ?")
            .bind((-t.amount).minor())
            .bind(transfer_id)
            .bind(t.id)
            .execute(&mut *db)
            .await?;
    }
    Ok(())
}

/// Read the row, apply the patch in memory, write it back, and fix up labels.
pub(super) async fn modify(
    db: &mut SqliteConnection,
    id: i64,
    patch: &TransactionPatch,
) -> Result<Transaction, StoreError> {
    let mut t = get(db, id).await?;
    if let Some(description) = &patch.description {
        t.description = description.clone();
    }
    if let Some(amount) = patch.amount {
        t.amount = amount;
    }
    if let Some(date) = patch.date {
        t.date = date;
    }
    if let Some(account) = &patch.account {
        t.account = account.clone();
    }
    if let Some(payee) = &patch.payee {
        t.payee = payee.clone();
    }
    if let Some(transfer_id) = t.transfer_id {
        keep_transfer_consistent(db, &t, transfer_id, patch).await?;
    }
    sqlx::query(
        "UPDATE transactions SET date = ?, amount = ?, description = ?, account = ?, payee = ? WHERE id = ?",
    )
    .bind(t.date)
    .bind(t.amount.minor())
    .bind(&t.description)
    .bind(&t.account)
    .bind(&t.payee)
    .bind(id)
    .execute(&mut *db)
    .await?;
    labels::detach(db, id, &patch.remove_labels).await?;
    labels::attach(db, id, &patch.add_labels).await?;
    get(db, id).await
}

/// Delete the transaction, plus the other side if it is part of a transfer.
/// Returns the ids that were removed.
pub(super) async fn delete(db: &mut SqliteConnection, id: i64) -> Result<Vec<i64>, StoreError> {
    let transfer_id = get(db, id).await?.transfer_id;
    let ids: Vec<i64> = match transfer_id {
        Some(transfer_id) => {
            sqlx::query_scalar("SELECT id FROM transactions WHERE transfer_id = ? ORDER BY id")
                .bind(transfer_id)
                .fetch_all(&mut *db)
                .await?
        }
        None => vec![id],
    };
    for id in &ids {
        sqlx::query("DELETE FROM transactions WHERE id = ?")
            .bind(id)
            .execute(&mut *db)
            .await?;
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::super::memory_store;
    use crate::{InsertSummary, Store, StoreError};
    use chrono::NaiveDate;
    use fca_core::{Filter, Kind, Money, NewTransaction, Origin, TransactionPatch};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn tx(date: NaiveDate, minor: i64, desc: &str, labels: &[&str]) -> NewTransaction {
        NewTransaction {
            date,
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            description: desc.into(),
            account: "cash".into(),
            payee: Some("Rewe".into()),
            origin: Origin::Manual,
            recurring_id: None,
            import_hash: None,
            labels: labels.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn insert_and_list_round_trip() {
        let store = memory_store().await;
        let id = store
            .insert_transaction(&tx(d(2026, 10, 1), -1250, "Groceries", &["food", "weekly"]))
            .await
            .unwrap();
        assert_eq!(id, Some(1));
        let all = store.list_transactions(&Filter::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].amount.minor(), -1250);
        assert_eq!(all[0].labels, ["food", "weekly"]);
        assert_eq!(all[0].payee.as_deref(), Some("Rewe"));
    }

    #[tokio::test]
    async fn filters_combine() {
        let store = memory_store().await;
        store
            .insert_transaction(&tx(d(2026, 10, 1), -1250, "Groceries", &["food"]))
            .await
            .unwrap();
        store
            .insert_transaction(&tx(d(2026, 10, 15), 250000, "Salary", &["work"]))
            .await
            .unwrap();
        store
            .insert_transaction(&tx(d(2026, 11, 2), -999, "Book", &["food", "fun"]))
            .await
            .unwrap();

        let count = |f: Filter| {
            let store = &store;
            async move { store.list_transactions(&f).await.unwrap().len() }
        };
        assert_eq!(
            count(Filter {
                labels: vec!["FOOD".into()],
                ..Filter::default()
            })
            .await,
            2
        );
        assert_eq!(
            count(Filter {
                labels: vec!["food".into(), "fun".into()],
                ..Filter::default()
            })
            .await,
            1
        );
        assert_eq!(
            count(Filter {
                kind: Some(Kind::Income),
                ..Filter::default()
            })
            .await,
            1
        );
        assert_eq!(
            count(Filter {
                from: Some(d(2026, 10, 2)),
                to: Some(d(2026, 10, 31)),
                ..Filter::default()
            })
            .await,
            1
        );
        assert_eq!(
            count(Filter {
                account: Some("CASH".into()),
                ..Filter::default()
            })
            .await,
            3
        );
        assert_eq!(
            count(Filter {
                origin: Some(Origin::Import),
                ..Filter::default()
            })
            .await,
            0
        );
    }

    #[tokio::test]
    async fn duplicate_import_hash_is_skipped() {
        let store = memory_store().await;
        let mut a = tx(d(2026, 10, 1), -100, "A", &[]);
        a.import_hash = Some("h1".into());
        let mut b = tx(d(2026, 10, 2), -200, "B", &[]);
        b.import_hash = Some("h2".into());
        let first = store
            .insert_transactions(&[a.clone(), b.clone()])
            .await
            .unwrap();
        assert_eq!(
            first,
            InsertSummary {
                inserted: 2,
                skipped: 0
            }
        );
        let second = store.insert_transactions(&[a, b]).await.unwrap();
        assert_eq!(
            second,
            InsertSummary {
                inserted: 0,
                skipped: 2
            }
        );
    }

    #[tokio::test]
    async fn modify_changes_fields_and_labels() {
        let store = memory_store().await;
        store
            .insert_transaction(&tx(d(2026, 10, 1), -1250, "Groceries", &["food", "old"]))
            .await
            .unwrap();
        let patch = TransactionPatch {
            description: Some("Weekly shop".into()),
            amount: Some(Money::from_minor(-1300)),
            payee: Some(None),
            add_labels: vec!["new".into()],
            remove_labels: vec!["OLD".into()],
            ..TransactionPatch::default()
        };
        let updated = store.modify_transaction(1, &patch).await.unwrap();
        assert_eq!(updated.description, "Weekly shop");
        assert_eq!(updated.amount.minor(), -1300);
        assert_eq!(updated.payee, None);
        assert_eq!(updated.labels, ["food", "new"]);
        assert_eq!(updated.date, d(2026, 10, 1));
    }

    #[tokio::test]
    async fn delete_and_not_found() {
        let store = memory_store().await;
        store
            .insert_transaction(&tx(d(2026, 10, 1), -100, "X", &["a"]))
            .await
            .unwrap();
        store.delete_transaction(1).await.unwrap();
        assert!(store
            .list_transactions(&Filter::default())
            .await
            .unwrap()
            .is_empty());
        assert!(
            store.list_labels().await.unwrap().is_empty(),
            "label links cascade"
        );
        assert!(matches!(
            store.delete_transaction(1).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(matches!(
            store
                .modify_transaction(9, &TransactionPatch::default())
                .await,
            Err(StoreError::NotFound { id: 9, .. })
        ));
    }

    #[tokio::test]
    async fn label_counts() {
        let store = memory_store().await;
        store
            .insert_transaction(&tx(d(2026, 10, 1), -100, "X", &["food", "fun"]))
            .await
            .unwrap();
        store
            .insert_transaction(&tx(d(2026, 10, 2), -100, "Y", &["food"]))
            .await
            .unwrap();
        let labels = store.list_labels().await.unwrap();
        let pairs: Vec<_> = labels.iter().map(|l| (l.name.as_str(), l.count)).collect();
        assert_eq!(pairs, [("food", 2), ("fun", 1)]);
    }
}

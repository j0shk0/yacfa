//! SQL for recurring entries, and `sync`, which persists what `fca-core` says is due.
//!
//! The decision of *which* dates are due is `fca_core::due_occurrences`; this file
//! only reads rules, inserts the resulting transactions, and bumps `last_generated`.

use chrono::NaiveDate;
use fca_core::{due_occurrences, Money, NewRecurring, Recurring, Schedule};
use sqlx::SqliteConnection;

use super::{labels, transactions, transfers};
use crate::StoreError;

const SELECT: &str = "SELECT r.id, r.description, r.amount, r.currency, r.account, r.payee, \
    r.transfer_to, r.schedule, r.start_date, r.end_date, r.last_generated, r.active, \
    (SELECT group_concat(l.name, char(31)) FROM recurring_labels rl \
     JOIN labels l ON l.id = rl.label_id WHERE rl.recurring_id = r.id) AS labels \
    FROM recurring r";

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    description: String,
    amount: i64,
    currency: String,
    account: String,
    payee: Option<String>,
    transfer_to: Option<String>,
    schedule: String,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    last_generated: Option<NaiveDate>,
    active: bool,
    labels: Option<String>,
}

impl Row {
    fn into_recurring(self) -> Result<Recurring, StoreError> {
        let schedule: Schedule = self
            .schedule
            .parse()
            .map_err(|e| StoreError::Corrupt(format!("{e}")))?;
        Ok(Recurring {
            id: self.id,
            description: self.description,
            amount: Money::from_minor(self.amount),
            currency: self.currency,
            account: self.account,
            payee: self.payee,
            transfer_to: self.transfer_to,
            schedule,
            start_date: self.start_date,
            end_date: self.end_date,
            last_generated: self.last_generated,
            active: self.active,
            labels: labels::split_concat(self.labels),
        })
    }
}

pub(super) async fn insert(
    db: &mut SqliteConnection,
    rec: &NewRecurring,
) -> Result<i64, StoreError> {
    rec.validate()
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
    let result = sqlx::query(
        "INSERT INTO recurring \
         (description, amount, currency, account, payee, transfer_to, schedule, start_date, end_date) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&rec.description)
    .bind(rec.amount.minor())
    .bind(&rec.currency)
    .bind(&rec.account)
    .bind(&rec.payee)
    .bind(&rec.transfer_to)
    .bind(rec.schedule.to_string())
    .bind(rec.start_date)
    .bind(rec.end_date)
    .execute(&mut *db)
    .await?;
    let id = result.last_insert_rowid();
    labels::attach_to_recurring(db, id, &rec.labels).await?;
    Ok(id)
}

pub(super) async fn list(db: &mut SqliteConnection) -> Result<Vec<Recurring>, StoreError> {
    let rows: Vec<Row> = sqlx::query_as(&format!("{SELECT} ORDER BY r.id"))
        .fetch_all(&mut *db)
        .await?;
    rows.into_iter().map(Row::into_recurring).collect()
}

pub(super) async fn delete(db: &mut SqliteConnection, id: i64) -> Result<(), StoreError> {
    let result = sqlx::query("DELETE FROM recurring WHERE id = ?")
        .bind(id)
        .execute(&mut *db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(StoreError::NotFound {
            what: "recurring entry",
            id,
        });
    }
    Ok(())
}

/// Insert every due occurrence of every recurring entry. Returns the number of
/// transactions created (a recurring transfer creates two per occurrence).
///
/// `(recurring_id, date)` is UNIQUE, so a duplicate is skipped rather than doubled,
/// and `last_generated` moves forward either way.
pub(super) async fn sync(db: &mut SqliteConnection, today: NaiveDate) -> Result<usize, StoreError> {
    let mut created = 0;
    for rec in list(db).await? {
        let due = due_occurrences(&rec, today);
        let Some(&newest) = due.last() else { continue };
        for date in due {
            // A duplicate (same entry and date) is skipped, including a transfer's other side.
            let Some(out_id) = transactions::insert_one(db, &rec.to_transaction(date)).await?
            else {
                continue;
            };
            created += 1;
            if let Some(incoming) = rec.incoming_leg(date) {
                let skipped = || StoreError::Corrupt("incoming side was not inserted".to_string());
                let in_id = transactions::insert_one(db, &incoming)
                    .await?
                    .ok_or_else(skipped)?;
                transfers::tie(db, out_id, in_id).await?;
                created += 1;
            }
        }
        sqlx::query("UPDATE recurring SET last_generated = ? WHERE id = ?")
            .bind(newest)
            .bind(rec.id)
            .execute(&mut *db)
            .await?;
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::super::memory_store;
    use crate::{Store, StoreError};
    use chrono::NaiveDate;
    use fca_core::{Filter, Money, NewRecurring, Origin};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn weekly(start: NaiveDate, end: Option<NaiveDate>) -> NewRecurring {
        NewRecurring {
            description: "Gym".into(),
            amount: Money::from_minor(-2000),
            currency: "EUR".into(),
            account: "giro".into(),
            payee: Some("FitCo".into()),
            transfer_to: None,
            schedule: "weekly".parse().unwrap(),
            start_date: start,
            end_date: end,
            labels: vec!["sport".into()],
        }
    }

    async fn recurring_rows(store: &impl Store) -> Vec<fca_core::Transaction> {
        let filter = Filter {
            origin: Some(Origin::Recurring),
            ..Filter::default()
        };
        store.list_transactions(&filter).await.unwrap()
    }

    #[tokio::test]
    async fn add_and_list_round_trip() {
        let store = memory_store().await;
        let id = store
            .add_recurring(&weekly(d(2026, 10, 1), Some(d(2026, 12, 1))))
            .await
            .unwrap();
        let all = store.list_recurring().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            (
                all[0].id,
                all[0].schedule.to_string(),
                all[0].labels.clone()
            ),
            (id, "1w".into(), vec!["sport".to_string()])
        );
        assert_eq!(
            (all[0].end_date, all[0].last_generated, all[0].active),
            (Some(d(2026, 12, 1)), None, true)
        );
    }

    #[tokio::test]
    async fn sync_is_idempotent() {
        let store = memory_store().await;
        store
            .add_recurring(&weekly(d(2026, 10, 1), None))
            .await
            .unwrap();
        assert_eq!(store.sync_recurring(d(2026, 10, 22)).await.unwrap(), 4);
        assert_eq!(store.sync_recurring(d(2026, 10, 22)).await.unwrap(), 0);
        assert_eq!(recurring_rows(&store).await.len(), 4);
        // Time passes: only the new occurrences are added.
        assert_eq!(store.sync_recurring(d(2026, 11, 5)).await.unwrap(), 2);
        let rec = &store.list_recurring().await.unwrap()[0];
        assert_eq!(rec.last_generated, Some(d(2026, 11, 5)));
    }

    #[tokio::test]
    async fn sync_respects_end_date_and_copies_fields() {
        let store = memory_store().await;
        store
            .add_recurring(&weekly(d(2026, 10, 1), Some(d(2026, 10, 10))))
            .await
            .unwrap();
        assert_eq!(store.sync_recurring(d(2027, 1, 1)).await.unwrap(), 2);
        let rows = recurring_rows(&store).await;
        assert_eq!(rows[1].date, d(2026, 10, 8));
        assert_eq!(
            (
                rows[0].account.as_str(),
                rows[0].labels.clone(),
                rows[0].recurring_id
            ),
            ("giro", vec!["sport".to_string()], Some(1))
        );
        assert_eq!(rows[0].amount.minor(), -2000);
    }

    #[tokio::test]
    async fn deleted_generated_rows_are_not_regenerated() {
        let store = memory_store().await;
        store
            .add_recurring(&weekly(d(2026, 10, 1), None))
            .await
            .unwrap();
        store.sync_recurring(d(2026, 10, 8)).await.unwrap();
        store.delete_transaction(1).await.unwrap();
        assert_eq!(store.sync_recurring(d(2026, 10, 8)).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn deleting_recurring_keeps_its_transactions() {
        let store = memory_store().await;
        let id = store
            .add_recurring(&weekly(d(2026, 10, 1), None))
            .await
            .unwrap();
        store.sync_recurring(d(2026, 10, 8)).await.unwrap();
        store.delete_recurring(id).await.unwrap();
        let rows = recurring_rows(&store).await;
        assert_eq!((rows.len(), rows[0].recurring_id), (2, None));
        assert!(matches!(
            store.delete_recurring(id).await,
            Err(StoreError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn recurring_transfer_creates_both_sides_once() {
        let store = memory_store().await;
        let rule = NewRecurring {
            transfer_to: Some("savings".into()),
            amount: Money::from_minor(-5000),
            ..weekly(d(2026, 10, 1), None)
        };
        store.add_recurring(&rule).await.unwrap();

        assert_eq!(
            store.sync_recurring(d(2026, 10, 8)).await.unwrap(),
            4,
            "2 occurrences, 2 sides each"
        );
        assert_eq!(store.sync_recurring(d(2026, 10, 8)).await.unwrap(), 0);
        let rows = recurring_rows(&store).await;
        assert_eq!(rows.len(), 4);
        // Sides of one occurrence share a transfer id and are in different accounts.
        let first: Vec<_> = rows.iter().filter(|t| t.date == d(2026, 10, 1)).collect();
        assert_eq!(first[0].transfer_id, first[1].transfer_id);
        assert!(first[0].transfer_id.is_some());
        let accounts: Vec<_> = first
            .iter()
            .map(|t| (t.account.as_str(), t.amount.minor()))
            .collect();
        assert_eq!(accounts, [("giro", -5000), ("savings", 5000)]);

        let kind = Filter {
            kind: Some(fca_core::Kind::Expense),
            ..Filter::default()
        };
        assert!(
            store.list_transactions(&kind).await.unwrap().is_empty(),
            "not an expense"
        );
        let balances: Vec<_> = store
            .balances(d(2026, 12, 31))
            .await
            .unwrap()
            .into_iter()
            .map(|b| (b.account, b.amount.minor()))
            .collect();
        assert_eq!(
            balances,
            [
                ("giro".to_string(), -10_000),
                ("savings".to_string(), 10_000)
            ]
        );
    }

    #[tokio::test]
    async fn invalid_recurring_transfer_is_rejected() {
        let store = memory_store().await;
        let same_account = NewRecurring {
            transfer_to: Some("GIRO".into()),
            ..weekly(d(2026, 10, 1), None)
        };
        assert!(matches!(
            store.add_recurring(&same_account).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(store.list_recurring().await.unwrap().is_empty());
    }
}

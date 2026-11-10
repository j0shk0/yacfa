//! SQL for transfers: create both sides, link two existing transactions, unlink.
//!
//! The rules (what makes a valid transfer) are in `fca_core::transfer`; this file
//! only stores the result. Deleting and modifying transfer sides is handled in
//! `transactions.rs`, next to the other changes to a transaction.

use fca_core::{check_linkable, NewTransfer};
use sqlx::SqliteConnection;

use super::transactions;
use crate::StoreError;

/// Insert both sides and tie them together. Returns `(outgoing id, incoming id)`.
pub(super) async fn add(
    db: &mut SqliteConnection,
    transfer: &NewTransfer,
) -> Result<(i64, i64), StoreError> {
    let (out, inc) = transfer
        .legs()
        .map_err(|e| StoreError::Invalid(e.to_string()))?;
    // Neither side has an import hash or recurring id, so inserting cannot be skipped.
    let skipped = || StoreError::Corrupt("transfer side was not inserted".to_string());
    let out_id = transactions::insert_one(db, &out)
        .await?
        .ok_or_else(skipped)?;
    let in_id = transactions::insert_one(db, &inc)
        .await?
        .ok_or_else(skipped)?;
    tie(db, out_id, in_id).await?;
    Ok((out_id, in_id))
}

/// Make two stored transactions the sides of one transfer (id = the outgoing side's id).
pub(super) async fn tie(
    db: &mut SqliteConnection,
    out_id: i64,
    in_id: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE transactions SET transfer_id = ? WHERE id IN (?, ?)")
        .bind(out_id)
        .bind(out_id)
        .bind(in_id)
        .execute(&mut *db)
        .await?;
    Ok(())
}

/// Mark two existing transactions (e.g. imported from two banks) as one transfer.
pub(super) async fn link(db: &mut SqliteConnection, a: i64, b: i64) -> Result<(), StoreError> {
    let (first, second) = (
        transactions::get(db, a).await?,
        transactions::get(db, b).await?,
    );
    check_linkable(&first, &second).map_err(|e| StoreError::Invalid(e.to_string()))?;
    tie(db, first.id, second.id).await
}

/// Turn both sides of the transfer that `id` belongs to back into ordinary transactions.
pub(super) async fn unlink(db: &mut SqliteConnection, id: i64) -> Result<(), StoreError> {
    let transfer_id = transactions::get(db, id)
        .await?
        .transfer_id
        .ok_or_else(|| {
            StoreError::Invalid(format!("transaction {id} is not part of a transfer"))
        })?;
    sqlx::query("UPDATE transactions SET transfer_id = NULL WHERE transfer_id = ?")
        .bind(transfer_id)
        .execute(&mut *db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::memory_store;
    use crate::{Store, StoreError};
    use chrono::NaiveDate;
    use fca_core::{Filter, Kind, Money, NewTransaction, NewTransfer, Origin, TransactionPatch};

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn transfer(minor: i64) -> NewTransfer {
        NewTransfer {
            date: d(1),
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            from: "giro".into(),
            to: "savings".into(),
            description: None,
            labels: vec!["saving".into()],
        }
    }

    fn imported(day: u32, account: &str, minor: i64, hash: &str) -> NewTransaction {
        NewTransaction {
            date: d(day),
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            description: "bank line".into(),
            account: account.into(),
            payee: None,
            origin: Origin::Import,
            recurring_id: None,
            import_hash: Some(hash.into()),
            labels: vec![],
        }
    }

    async fn of_kind(store: &impl Store, kind: Kind) -> usize {
        let filter = Filter {
            kind: Some(kind),
            ..Filter::default()
        };
        store.list_transactions(&filter).await.unwrap().len()
    }

    #[tokio::test]
    async fn transfer_creates_two_linked_sides_that_are_not_income_or_expense() {
        let store = memory_store().await;
        assert_eq!(store.add_transfer(&transfer(20_000)).await.unwrap(), (1, 2));
        let all = store.list_transactions(&Filter::default()).await.unwrap();
        assert_eq!(
            (all[0].amount.minor(), all[1].amount.minor()),
            (-20_000, 20_000)
        );
        assert_eq!((all[0].transfer_id, all[1].transfer_id), (Some(1), Some(1)));
        assert_eq!(all[1].labels, ["saving"]);

        assert_eq!(of_kind(&store, Kind::Transfer).await, 2);
        assert_eq!(of_kind(&store, Kind::Income).await, 0);
        assert_eq!(of_kind(&store, Kind::Expense).await, 0);
    }

    #[tokio::test]
    async fn transfers_move_balances_but_not_the_total() {
        let store = memory_store().await;
        store.add_transfer(&transfer(20_000)).await.unwrap();
        let balances = store.balances(d(31)).await.unwrap();
        let amounts: Vec<_> = balances
            .iter()
            .map(|b| (b.account.as_str(), b.amount.minor()))
            .collect();
        assert_eq!(amounts, [("giro", -20_000), ("savings", 20_000)]);
    }

    #[tokio::test]
    async fn invalid_transfer_is_rejected() {
        let store = memory_store().await;
        let mut same = transfer(100);
        same.to = "GIRO".into();
        assert!(matches!(
            store.add_transfer(&same).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(store
            .list_transactions(&Filter::default())
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn deleting_one_side_deletes_both() {
        let store = memory_store().await;
        store.add_transfer(&transfer(100)).await.unwrap();
        assert_eq!(store.delete_transaction(2).await.unwrap(), [1, 2]);
        assert!(store
            .list_transactions(&Filter::default())
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn changing_the_amount_of_one_side_mirrors_it() {
        let store = memory_store().await;
        store.add_transfer(&transfer(20_000)).await.unwrap();
        let patch = TransactionPatch {
            amount: Some(Money::from_minor(-25_000)),
            ..TransactionPatch::default()
        };
        store.modify_transaction(1, &patch).await.unwrap();
        let all = store.list_transactions(&Filter::default()).await.unwrap();
        assert_eq!(
            (all[0].amount.minor(), all[1].amount.minor()),
            (-25_000, 25_000)
        );
    }

    #[tokio::test]
    async fn moving_one_side_into_the_other_account_is_rejected() {
        let store = memory_store().await;
        store.add_transfer(&transfer(100)).await.unwrap();
        let patch = TransactionPatch {
            account: Some("Savings".into()),
            ..TransactionPatch::default()
        };
        assert!(matches!(
            store.modify_transaction(1, &patch).await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn link_and_unlink_imported_transactions() {
        let store = memory_store().await;
        store
            .insert_transaction(&imported(1, "giro", -5_000, "h1"))
            .await
            .unwrap();
        store
            .insert_transaction(&imported(3, "credit", 5_000, "h2"))
            .await
            .unwrap();
        store
            .insert_transaction(&imported(3, "credit", 4_000, "h3"))
            .await
            .unwrap();

        assert!(
            matches!(
                store.link_transfers(1, 3).await,
                Err(StoreError::Invalid(_))
            ),
            "amounts differ"
        );
        store.link_transfers(1, 2).await.unwrap();
        assert_eq!(of_kind(&store, Kind::Transfer).await, 2);
        assert!(
            matches!(
                store.link_transfers(1, 2).await,
                Err(StoreError::Invalid(_))
            ),
            "already linked"
        );

        store.unlink_transfer(2).await.unwrap();
        assert_eq!(of_kind(&store, Kind::Transfer).await, 0);
        assert!(matches!(
            store.unlink_transfer(2).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            store.link_transfers(1, 99).await,
            Err(StoreError::NotFound { .. })
        ));
    }
}

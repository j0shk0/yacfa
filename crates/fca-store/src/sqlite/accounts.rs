//! SQL for account settings and balances.
//!
//! SQL only sums transactions; adding the opening balance is `fca_core::compute_balances`.

use chrono::NaiveDate;
use fca_core::{compute_balances, Account, Balance, Money};
use sqlx::SqliteConnection;

use crate::StoreError;

/// Create or replace the settings of one account.
pub(super) async fn set(db: &mut SqliteConnection, account: &Account) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO accounts (name, currency, opening_balance) VALUES (?, ?, ?) \
         ON CONFLICT (name) DO UPDATE SET \
         currency = excluded.currency, opening_balance = excluded.opening_balance",
    )
    .bind(&account.name)
    .bind(&account.currency)
    .bind(account.opening_balance.minor())
    .execute(&mut *db)
    .await?;
    Ok(())
}

pub(super) async fn list(db: &mut SqliteConnection) -> Result<Vec<Account>, StoreError> {
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT name, currency, opening_balance FROM accounts ORDER BY name")
            .fetch_all(&mut *db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(name, currency, opening)| Account {
            name,
            currency,
            opening_balance: Money::from_minor(opening),
        })
        .collect())
}

/// Balances of all accounts including transactions dated up to and including `asof`.
pub(super) async fn balances(
    db: &mut SqliteConnection,
    asof: NaiveDate,
) -> Result<Vec<Balance>, StoreError> {
    // Names differ only by case in practice ("Cash" vs "cash"), so group case-insensitively.
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT MIN(account), currency, SUM(amount) FROM transactions \
         WHERE date <= ? GROUP BY lower(account), currency",
    )
    .bind(asof)
    .fetch_all(&mut *db)
    .await?;
    let sums: Vec<Balance> = rows
        .into_iter()
        .map(|(account, currency, sum)| Balance {
            account,
            currency,
            amount: Money::from_minor(sum),
        })
        .collect();
    Ok(compute_balances(&list(db).await?, &sums))
}

#[cfg(test)]
mod tests {
    use super::super::memory_store;
    use crate::Store;
    use chrono::NaiveDate;
    use fca_core::{Account, Money, NewTransaction, Origin};

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn tx(day: u32, account: &str, minor: i64) -> NewTransaction {
        NewTransaction {
            date: d(day),
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            description: "x".into(),
            account: account.into(),
            payee: None,
            origin: Origin::Manual,
            recurring_id: None,
            import_hash: None,
            labels: vec![],
        }
    }

    fn acct(name: &str, opening: i64) -> Account {
        Account {
            name: name.into(),
            currency: "EUR".into(),
            opening_balance: Money::from_minor(opening),
        }
    }

    async fn amounts(store: &impl Store, asof: NaiveDate) -> Vec<(String, i64)> {
        let balances = store.balances(asof).await.unwrap();
        balances
            .into_iter()
            .map(|b| (b.account, b.amount.minor()))
            .collect()
    }

    #[tokio::test]
    async fn balance_is_opening_plus_transactions_up_to_the_date() {
        let store = memory_store().await;
        store.set_account(&acct("giro", 100_000)).await.unwrap();
        store
            .insert_transaction(&tx(1, "giro", -2_000))
            .await
            .unwrap();
        store
            .insert_transaction(&tx(10, "GIRO", 50_000))
            .await
            .unwrap();
        store.insert_transaction(&tx(5, "cash", 700)).await.unwrap();

        assert_eq!(
            amounts(&store, d(31)).await,
            [("cash".into(), 700), ("giro".into(), 148_000)]
        );
        assert_eq!(amounts(&store, d(4)).await, [("giro".into(), 98_000)]);
    }

    #[tokio::test]
    async fn set_account_replaces_the_previous_settings() {
        let store = memory_store().await;
        store.set_account(&acct("Giro", 100)).await.unwrap();
        store.set_account(&acct("giro", 250)).await.unwrap();
        let accounts = store.list_accounts().await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].opening_balance.minor(), 250);
    }
}

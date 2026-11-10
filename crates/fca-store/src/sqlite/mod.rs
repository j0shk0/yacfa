//! SQLite implementation of `Store`: connection setup, migrations, and dispatch.
//!
//! The SQL itself lives in the sibling files (one per topic) as functions that
//! take a connection, so several of them can share one DB transaction.
//! Must NOT hold business rules.

mod accounts;
mod labels;
mod recurring;
mod transactions;
mod transfers;

use std::str::FromStr;

use chrono::NaiveDate;
use fca_core::{
    Account, Balance, Filter, NewRecurring, NewTransaction, NewTransfer, Recurring, Transaction,
    TransactionPatch,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::{InsertSummary, LabelCount, Store, StoreError};

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (creating if needed) a database and apply pending migrations.
    ///
    /// `target` is a file path, or a `sqlite:` URL such as `sqlite::memory:`.
    pub async fn connect(target: &str) -> Result<Self, StoreError> {
        let options = if target.starts_with("sqlite:") {
            SqliteConnectOptions::from_str(target)?
        } else {
            SqliteConnectOptions::new().filename(target)
        }
        .create_if_missing(true)
        .foreign_keys(true);

        // One connection is plenty for a CLI, and it keeps `sqlite::memory:` on a
        // single database. Never expire it, or an in-memory database would vanish.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations/sqlite").run(&pool).await?;
        Ok(SqliteStore { pool })
    }
}

impl Store for SqliteStore {
    async fn insert_transaction(&self, tx: &NewTransaction) -> Result<Option<i64>, StoreError> {
        let mut db = self.pool.begin().await?;
        let id = transactions::insert_one(&mut db, tx).await?;
        db.commit().await?;
        Ok(id)
    }

    async fn insert_transactions(
        &self,
        txs: &[NewTransaction],
    ) -> Result<InsertSummary, StoreError> {
        let mut db = self.pool.begin().await?;
        let mut summary = InsertSummary::default();
        for tx in txs {
            match transactions::insert_one(&mut db, tx).await? {
                Some(_) => summary.inserted += 1,
                None => summary.skipped += 1,
            }
        }
        db.commit().await?;
        Ok(summary)
    }

    async fn list_transactions(&self, filter: &Filter) -> Result<Vec<Transaction>, StoreError> {
        let mut db = self.pool.acquire().await?;
        transactions::list(&mut db, filter).await
    }

    async fn modify_transaction(
        &self,
        id: i64,
        patch: &TransactionPatch,
    ) -> Result<Transaction, StoreError> {
        let mut db = self.pool.begin().await?;
        let updated = transactions::modify(&mut db, id, patch).await?;
        db.commit().await?;
        Ok(updated)
    }

    async fn delete_transaction(&self, id: i64) -> Result<Vec<i64>, StoreError> {
        let mut db = self.pool.begin().await?;
        let deleted = transactions::delete(&mut db, id).await?;
        db.commit().await?;
        Ok(deleted)
    }

    async fn list_labels(&self) -> Result<Vec<LabelCount>, StoreError> {
        let mut db = self.pool.acquire().await?;
        labels::list_with_counts(&mut db).await
    }

    async fn add_transfer(&self, transfer: &NewTransfer) -> Result<(i64, i64), StoreError> {
        let mut db = self.pool.begin().await?;
        let ids = transfers::add(&mut db, transfer).await?;
        db.commit().await?;
        Ok(ids)
    }

    async fn link_transfers(&self, a: i64, b: i64) -> Result<(), StoreError> {
        let mut db = self.pool.begin().await?;
        transfers::link(&mut db, a, b).await?;
        db.commit().await?;
        Ok(())
    }

    async fn unlink_transfer(&self, id: i64) -> Result<(), StoreError> {
        let mut db = self.pool.begin().await?;
        transfers::unlink(&mut db, id).await?;
        db.commit().await?;
        Ok(())
    }

    async fn set_account(&self, account: &Account) -> Result<(), StoreError> {
        let mut db = self.pool.acquire().await?;
        accounts::set(&mut db, account).await
    }

    async fn list_accounts(&self) -> Result<Vec<Account>, StoreError> {
        let mut db = self.pool.acquire().await?;
        accounts::list(&mut db).await
    }

    async fn balances(&self, asof: NaiveDate) -> Result<Vec<Balance>, StoreError> {
        let mut db = self.pool.acquire().await?;
        accounts::balances(&mut db, asof).await
    }

    async fn add_recurring(&self, rec: &NewRecurring) -> Result<i64, StoreError> {
        let mut db = self.pool.begin().await?;
        let id = recurring::insert(&mut db, rec).await?;
        db.commit().await?;
        Ok(id)
    }

    async fn list_recurring(&self) -> Result<Vec<Recurring>, StoreError> {
        let mut db = self.pool.acquire().await?;
        recurring::list(&mut db).await
    }

    async fn delete_recurring(&self, id: i64) -> Result<(), StoreError> {
        let mut db = self.pool.acquire().await?;
        recurring::delete(&mut db, id).await
    }

    async fn sync_recurring(&self, today: NaiveDate) -> Result<usize, StoreError> {
        let mut db = self.pool.begin().await?;
        let created = recurring::sync(&mut db, today).await?;
        db.commit().await?;
        Ok(created)
    }
}

/// Test helper: a fresh in-memory store.
#[cfg(test)]
pub(crate) async fn memory_store() -> SqliteStore {
    SqliteStore::connect("sqlite::memory:").await.unwrap()
}

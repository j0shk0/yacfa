//! Persistence for yafca: the `Store` trait and its SQLite implementation.
//!
//! The trait is the only thing the CLI (and a future TUI) depends on, so a
//! Postgres implementation can be added next to `sqlite/` without touching them.
//! Must NOT contain business rules (those live in `fca-core`) or print anything.

pub mod sqlite;

use chrono::NaiveDate;
use fca_core::{
    Account, Balance, Filter, NewRecurring, NewTransaction, NewTransfer, Recurring, Transaction,
    TransactionPatch,
};
use thiserror::Error;

pub use sqlite::SqliteStore;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("{what} {id} not found")]
    NotFound { what: &'static str, id: i64 },
    /// The request breaks a rule (e.g. linking transactions that do not cancel out).
    #[error("{0}")]
    Invalid(String),
    #[error("corrupt data in database: {0}")]
    Corrupt(String),
}

/// How many rows a batch insert added and how many it skipped as duplicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InsertSummary {
    pub inserted: usize,
    pub skipped: usize,
}

/// A label and the number of transactions that carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelCount {
    pub name: String,
    pub count: i64,
}

/// Everything the application needs from storage.
///
/// `async fn` in a public trait is fine here: callers use concrete types on a
/// single-threaded runtime, so no `Send` bound is needed.
#[allow(async_fn_in_trait)]
pub trait Store {
    /// Insert one transaction. `None` means it was skipped as a duplicate
    /// (same `import_hash`, or same recurring entry and date).
    async fn insert_transaction(&self, tx: &NewTransaction) -> Result<Option<i64>, StoreError>;

    /// Insert many transactions atomically, skipping duplicates.
    async fn insert_transactions(
        &self,
        txs: &[NewTransaction],
    ) -> Result<InsertSummary, StoreError>;

    /// Transactions matching `filter`, oldest first.
    async fn list_transactions(&self, filter: &Filter) -> Result<Vec<Transaction>, StoreError>;

    /// Apply `patch` to transaction `id` and return the updated row.
    async fn modify_transaction(
        &self,
        id: i64,
        patch: &TransactionPatch,
    ) -> Result<Transaction, StoreError>;

    /// Delete a transaction and return the ids actually removed: deleting one
    /// side of a transfer also deletes the other side.
    async fn delete_transaction(&self, id: i64) -> Result<Vec<i64>, StoreError>;

    /// Labels in use, with counts, sorted by name.
    async fn list_labels(&self) -> Result<Vec<LabelCount>, StoreError>;

    /// Store both sides of a transfer. Returns `(outgoing id, incoming id)`.
    async fn add_transfer(&self, transfer: &NewTransfer) -> Result<(i64, i64), StoreError>;

    /// Make two existing transactions the two sides of one transfer.
    async fn link_transfers(&self, a: i64, b: i64) -> Result<(), StoreError>;

    /// Turn a transfer (both sides) back into ordinary transactions.
    async fn unlink_transfer(&self, id: i64) -> Result<(), StoreError>;

    /// Create or replace the settings (currency, opening balance) of an account.
    async fn set_account(&self, account: &Account) -> Result<(), StoreError>;

    async fn list_accounts(&self) -> Result<Vec<Account>, StoreError>;

    /// Balance per account and currency, counting transactions dated up to `asof`.
    async fn balances(&self, asof: NaiveDate) -> Result<Vec<Balance>, StoreError>;

    async fn add_recurring(&self, rec: &NewRecurring) -> Result<i64, StoreError>;

    async fn list_recurring(&self) -> Result<Vec<Recurring>, StoreError>;

    /// Delete a recurring entry. Transactions it already generated are kept.
    async fn delete_recurring(&self, id: i64) -> Result<(), StoreError>;

    /// Persist every recurring occurrence that is due by `today` and not yet stored.
    /// Safe to call repeatedly: running it twice with the same `today` adds nothing.
    /// Returns how many transactions were created.
    async fn sync_recurring(&self, today: NaiveDate) -> Result<usize, StoreError>;
}

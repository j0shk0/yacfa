//! Domain types and rules for yafca: money, schedules, transactions, filters.
//!
//! Must NOT do any IO, touch a database, or use async. Everything here is pure,
//! so it can be reviewed by reading it and checked by its unit tests.

pub mod account;
pub mod filter;
pub mod money;
pub mod recurring;
pub mod schedule;
pub mod transaction;
pub mod transfer;

pub use account::{compute_balances, Account, Balance};
pub use filter::{Filter, Kind};
pub use money::Money;
pub use recurring::{due_occurrences, NewRecurring, Recurring};
pub use schedule::{Schedule, Unit};
pub use transaction::{NewTransaction, Origin, Transaction, TransactionPatch};
pub use transfer::{check_linkable, same_account, NewTransfer, TransferError};

/// Currency used when the user does not give one.
pub const DEFAULT_CURRENCY: &str = "EUR";

/// Account used when the user does not give one.
pub const DEFAULT_ACCOUNT: &str = "default";

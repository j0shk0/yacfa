//! Transaction types: what is stored, what is inserted, and what `modify` may change.
//!
//! Amounts are signed: negative is an expense, positive is income.
//! Must NOT contain storage or parsing logic.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use thiserror::Error;

use crate::Money;

/// Where a transaction came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Manual,
    Import,
    Recurring,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown origin {0:?} (expected manual, import or recurring)")]
pub struct ParseOriginError(String);

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Manual => "manual",
            Origin::Import => "import",
            Origin::Recurring => "recurring",
        }
    }
}

impl FromStr for Origin {
    type Err = ParseOriginError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manual" => Ok(Origin::Manual),
            "import" => Ok(Origin::Import),
            "recurring" => Ok(Origin::Recurring),
            _ => Err(ParseOriginError(s.to_string())),
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A transaction that is not stored yet (no id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTransaction {
    pub date: NaiveDate,
    pub amount: Money,
    pub currency: String,
    pub description: String,
    /// Where the money moved (cash, giro, ...).
    pub account: String,
    /// The other party, if known.
    pub payee: Option<String>,
    pub origin: Origin,
    pub recurring_id: Option<i64>,
    /// Set by the importer so re-importing the same file adds nothing.
    pub import_hash: Option<String>,
    pub labels: Vec<String>,
}

/// A stored transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: i64,
    pub date: NaiveDate,
    pub amount: Money,
    pub currency: String,
    pub description: String,
    pub account: String,
    pub payee: Option<String>,
    pub origin: Origin,
    pub recurring_id: Option<i64>,
    /// Both sides of a transfer share this id; `None` for income and expenses.
    pub transfer_id: Option<i64>,
    pub labels: Vec<String>,
}

/// Changes applied by `modify`. `None` / empty means "leave unchanged".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransactionPatch {
    pub description: Option<String>,
    pub amount: Option<Money>,
    pub date: Option<NaiveDate>,
    pub account: Option<String>,
    /// `Some(None)` clears the payee.
    pub payee: Option<Option<String>>,
    pub add_labels: Vec<String>,
    pub remove_labels: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_round_trips() {
        for o in [Origin::Manual, Origin::Import, Origin::Recurring] {
            assert_eq!(o.to_string().parse::<Origin>().unwrap(), o);
        }
        assert!("bank".parse::<Origin>().is_err());
    }
}

//! Transfers: money moving between two of your own accounts.
//!
//! A transfer is stored as two ordinary transactions that share a `transfer_id`:
//! an outgoing one (negative) and an incoming one (positive). The two always sum
//! to zero, so transfers never change your net worth and are not income or expense.
//! Must NOT do IO; the store persists what `legs` and `check_linkable` describe.

use chrono::NaiveDate;
use thiserror::Error;

use crate::{Money, NewTransaction, Origin, Transaction};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransferError {
    #[error("transfer amount must be greater than zero")]
    NotPositive,
    #[error("a transfer needs two different accounts")]
    SameAccount,
    #[error("transaction {0} is already part of a transfer")]
    AlreadyLinked(i64),
    #[error("transactions {0} and {1} use different currencies")]
    CurrencyMismatch(i64, i64),
    #[error("transactions {0} and {1} must have equal amounts with opposite signs")]
    AmountMismatch(i64, i64),
}

/// A transfer that is not stored yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTransfer {
    pub date: NaiveDate,
    /// The amount moved; must be positive.
    pub amount: Money,
    pub currency: String,
    pub from: String,
    pub to: String,
    /// Used for both sides; defaults to "Transfer to X" / "Transfer from Y".
    pub description: Option<String>,
    pub labels: Vec<String>,
}

impl NewTransfer {
    /// The two transactions to store: `(outgoing from `from`, incoming to `to`)`.
    pub fn legs(&self) -> Result<(NewTransaction, NewTransaction), TransferError> {
        if self.amount.minor() <= 0 {
            return Err(TransferError::NotPositive);
        }
        if same_account(&self.from, &self.to) {
            return Err(TransferError::SameAccount);
        }
        let leg = |account: &str, amount: Money, default_description: String| NewTransaction {
            date: self.date,
            amount,
            currency: self.currency.clone(),
            description: self.description.clone().unwrap_or(default_description),
            account: account.to_string(),
            payee: None,
            origin: Origin::Manual,
            recurring_id: None,
            import_hash: None,
            labels: self.labels.clone(),
        };
        Ok((
            leg(&self.from, -self.amount, format!("Transfer to {}", self.to)),
            leg(
                &self.to,
                self.amount,
                format!("Transfer from {}", self.from),
            ),
        ))
    }
}

/// Can these two existing transactions be the two sides of one transfer?
///
/// Dates may differ (banks book at different times); everything else must match up.
pub fn check_linkable(a: &Transaction, b: &Transaction) -> Result<(), TransferError> {
    for t in [a, b] {
        if t.transfer_id.is_some() {
            return Err(TransferError::AlreadyLinked(t.id));
        }
    }
    if same_account(&a.account, &b.account) {
        return Err(TransferError::SameAccount);
    }
    if a.currency != b.currency {
        return Err(TransferError::CurrencyMismatch(a.id, b.id));
    }
    if a.amount.minor() == 0 || a.amount + b.amount != Money::default() {
        return Err(TransferError::AmountMismatch(a.id, b.id));
    }
    Ok(())
}

/// Account names are compared case-insensitively everywhere.
pub fn same_account(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn transfer(minor: i64, from: &str, to: &str) -> NewTransfer {
        NewTransfer {
            date: d(1),
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            from: from.into(),
            to: to.into(),
            description: None,
            labels: vec!["saving".into()],
        }
    }

    fn tx(id: i64, account: &str, minor: i64) -> Transaction {
        Transaction {
            id,
            date: d(1),
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            description: "x".into(),
            account: account.into(),
            payee: None,
            origin: Origin::Import,
            recurring_id: None,
            transfer_id: None,
            labels: vec![],
        }
    }

    #[test]
    fn legs_are_opposite_and_named() {
        let (out, inc) = transfer(20_000, "giro", "savings").legs().unwrap();
        assert_eq!(
            (out.account.as_str(), out.amount.minor()),
            ("giro", -20_000)
        );
        assert_eq!(
            (inc.account.as_str(), inc.amount.minor()),
            ("savings", 20_000)
        );
        assert_eq!(out.description, "Transfer to savings");
        assert_eq!(inc.description, "Transfer from giro");
        assert_eq!(out.labels, ["saving"]);
    }

    #[test]
    fn explicit_description_is_used_on_both_sides() {
        let mut t = transfer(100, "a", "b");
        t.description = Some("Rainy day".into());
        let (out, inc) = t.legs().unwrap();
        assert_eq!(
            (out.description.as_str(), inc.description.as_str()),
            ("Rainy day", "Rainy day")
        );
    }

    #[test]
    fn rejects_bad_transfers() {
        assert_eq!(
            transfer(0, "a", "b").legs().unwrap_err(),
            TransferError::NotPositive
        );
        assert_eq!(
            transfer(-5, "a", "b").legs().unwrap_err(),
            TransferError::NotPositive
        );
        assert_eq!(
            transfer(5, "Giro", "giro").legs().unwrap_err(),
            TransferError::SameAccount
        );
    }

    #[test]
    fn linkable_when_amounts_cancel_across_accounts() {
        assert!(check_linkable(&tx(1, "giro", -5_000), &tx(2, "credit", 5_000)).is_ok());
    }

    #[test]
    fn link_rejections() {
        let (a, b) = (tx(1, "giro", -5_000), tx(2, "credit", 5_000));
        assert_eq!(
            check_linkable(&a, &tx(2, "GIRO", 5_000)),
            Err(TransferError::SameAccount)
        );
        assert_eq!(
            check_linkable(&a, &tx(2, "credit", 4_000)),
            Err(TransferError::AmountMismatch(1, 2))
        );
        assert_eq!(
            check_linkable(&a, &tx(2, "credit", -5_000)),
            Err(TransferError::AmountMismatch(1, 2))
        );
        let mut usd = b.clone();
        usd.currency = "USD".into();
        assert_eq!(
            check_linkable(&a, &usd),
            Err(TransferError::CurrencyMismatch(1, 2))
        );
        let mut linked = b;
        linked.transfer_id = Some(9);
        assert_eq!(
            check_linkable(&a, &linked),
            Err(TransferError::AlreadyLinked(2))
        );
    }
}

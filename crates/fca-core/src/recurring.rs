//! Recurring entries (rent, salary, subscriptions) and the rule for which dates are due.
//!
//! `due_occurrences` is pure: it decides *what* to generate. The store only
//! persists the result. Must NOT touch the database or read the clock.

use chrono::NaiveDate;

use crate::{same_account, Money, NewTransaction, Origin, Schedule, TransferError};

/// A recurring entry that is not stored yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRecurring {
    pub description: String,
    pub amount: Money,
    pub currency: String,
    pub account: String,
    pub payee: Option<String>,
    /// `Some(account)` makes this a recurring transfer: `amount` (negative) leaves
    /// `account` and the same amount arrives in this account.
    pub transfer_to: Option<String>,
    pub schedule: Schedule,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub labels: Vec<String>,
}

impl NewRecurring {
    /// Check the rules of recurring transfers; ordinary entries always pass.
    pub fn validate(&self) -> Result<(), TransferError> {
        let Some(to) = &self.transfer_to else {
            return Ok(());
        };
        if self.amount.minor() >= 0 {
            return Err(TransferError::NotPositive);
        }
        if same_account(&self.account, to) {
            return Err(TransferError::SameAccount);
        }
        Ok(())
    }
}

/// A stored recurring entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurring {
    pub id: i64,
    pub description: String,
    pub amount: Money,
    pub currency: String,
    pub account: String,
    pub payee: Option<String>,
    /// See `NewRecurring::transfer_to`.
    pub transfer_to: Option<String>,
    pub schedule: Schedule,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    /// Latest date for which a transaction was already generated.
    pub last_generated: Option<NaiveDate>,
    pub active: bool,
    pub labels: Vec<String>,
}

impl Recurring {
    /// The transaction this entry produces on `date`. For a recurring transfer
    /// this is the outgoing side; see `incoming_leg` for the other one.
    pub fn to_transaction(&self, date: NaiveDate) -> NewTransaction {
        NewTransaction {
            date,
            amount: self.amount,
            currency: self.currency.clone(),
            description: self.description.clone(),
            account: self.account.clone(),
            payee: if self.transfer_to.is_some() {
                None
            } else {
                self.payee.clone()
            },
            origin: Origin::Recurring,
            recurring_id: Some(self.id),
            import_hash: None,
            labels: self.labels.clone(),
        }
    }

    /// For a recurring transfer, the incoming side on `date`; `None` otherwise.
    ///
    /// It deliberately has no `recurring_id`: the store allows only one transaction
    /// per recurring entry and date, and that slot belongs to the outgoing side.
    /// Syncing is still idempotent, because the incoming side is only created
    /// together with a newly created outgoing side.
    pub fn incoming_leg(&self, date: NaiveDate) -> Option<NewTransaction> {
        let to = self.transfer_to.as_ref()?;
        Some(NewTransaction {
            amount: -self.amount,
            account: to.clone(),
            recurring_id: None,
            ..self.to_transaction(date)
        })
    }
}

/// Dates on which `rec` should have produced a transaction by `today` but has not yet.
///
/// Inactive entries produce nothing. Dates after `end_date` or after `today` are
/// never due, and dates up to `last_generated` are already done.
pub fn due_occurrences(rec: &Recurring, today: NaiveDate) -> Vec<NaiveDate> {
    if !rec.active {
        return Vec::new();
    }
    let limit = rec.end_date.map_or(today, |end| end.min(today));
    rec.schedule
        .occurrences_until(rec.start_date, limit)
        .into_iter()
        .filter(|d| rec.last_generated.is_none_or(|last| *d > last))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn weekly_from(start: NaiveDate) -> Recurring {
        Recurring {
            id: 1,
            description: "Gym".into(),
            amount: Money::from_minor(-2000),
            currency: "EUR".into(),
            account: "giro".into(),
            payee: None,
            transfer_to: None,
            schedule: "weekly".parse().unwrap(),
            start_date: start,
            end_date: None,
            last_generated: None,
            active: true,
            labels: vec![],
        }
    }

    #[test]
    fn all_due_up_to_today_inclusive() {
        let rec = weekly_from(d(2026, 10, 1));
        assert_eq!(due_occurrences(&rec, d(2026, 10, 22)).len(), 4);
    }

    #[test]
    fn skips_already_generated() {
        let mut rec = weekly_from(d(2026, 10, 1));
        rec.last_generated = Some(d(2026, 10, 15));
        assert_eq!(
            due_occurrences(&rec, d(2026, 10, 29)),
            [d(2026, 10, 22), d(2026, 10, 29)]
        );
    }

    #[test]
    fn respects_end_date_and_future_start() {
        let mut rec = weekly_from(d(2026, 10, 1));
        rec.end_date = Some(d(2026, 10, 10));
        assert_eq!(
            due_occurrences(&rec, d(2026, 12, 1)),
            [d(2026, 10, 1), d(2026, 10, 8)]
        );
        assert!(due_occurrences(&weekly_from(d(2027, 1, 1)), d(2026, 12, 1)).is_empty());
    }

    #[test]
    fn inactive_produces_nothing() {
        let mut rec = weekly_from(d(2026, 10, 1));
        rec.active = false;
        assert!(due_occurrences(&rec, d(2026, 12, 1)).is_empty());
    }

    #[test]
    fn generated_transaction_links_back() {
        let tx = weekly_from(d(2026, 10, 1)).to_transaction(d(2026, 10, 8));
        assert_eq!(
            (tx.origin, tx.recurring_id, tx.date),
            (Origin::Recurring, Some(1), d(2026, 10, 8))
        );
    }

    fn transfer_rule() -> Recurring {
        Recurring {
            transfer_to: Some("savings".into()),
            payee: Some("ignored".into()),
            labels: vec!["saving".into()],
            ..weekly_from(d(2026, 10, 1))
        }
    }

    #[test]
    fn recurring_transfer_has_an_outgoing_and_an_incoming_side() {
        let rule = transfer_rule();
        let out = rule.to_transaction(d(2026, 10, 8));
        let incoming = rule.incoming_leg(d(2026, 10, 8)).unwrap();
        assert_eq!(
            (out.account.as_str(), out.amount.minor(), out.payee),
            ("giro", -2000, None)
        );
        assert_eq!(
            (incoming.account.as_str(), incoming.amount.minor()),
            ("savings", 2000)
        );
        assert_eq!(
            (incoming.origin, incoming.recurring_id),
            (Origin::Recurring, None)
        );
        assert_eq!(
            (incoming.description, incoming.labels),
            (out.description, out.labels)
        );
        assert!(weekly_from(d(2026, 10, 1))
            .incoming_leg(d(2026, 10, 8))
            .is_none());
    }

    #[test]
    fn validates_recurring_transfers() {
        let new = |amount: i64, to: &str| NewRecurring {
            description: "Save".into(),
            amount: Money::from_minor(amount),
            currency: "EUR".into(),
            account: "giro".into(),
            payee: None,
            transfer_to: Some(to.into()),
            schedule: "monthly".parse().unwrap(),
            start_date: d(2026, 10, 1),
            end_date: None,
            labels: vec![],
        };
        assert!(new(-500, "savings").validate().is_ok());
        assert_eq!(
            new(500, "savings").validate(),
            Err(TransferError::NotPositive)
        );
        assert_eq!(
            new(-500, "GIRO").validate(),
            Err(TransferError::SameAccount)
        );
        let plain = NewRecurring {
            transfer_to: None,
            amount: Money::from_minor(500),
            ..new(0, "x")
        };
        assert!(plain.validate().is_ok());
    }
}

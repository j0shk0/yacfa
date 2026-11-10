//! `Filter`: the criteria `list` uses to select transactions.
//!
//! Plain data; the store turns it into a query. Must NOT parse user input
//! (that is the CLI's `tokens.rs`) or run queries.

use chrono::NaiveDate;

use crate::Origin;

/// What a transaction is. Income and expense are decided by the sign of the
/// amount and never include transfers between your own accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Income,
    Expense,
    Transfer,
}

/// All fields are optional; an empty filter matches everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// A transaction must carry *all* of these labels.
    pub labels: Vec<String>,
    pub account: Option<String>,
    pub payee: Option<String>,
    /// Inclusive.
    pub from: Option<NaiveDate>,
    /// Inclusive.
    pub to: Option<NaiveDate>,
    pub origin: Option<Origin>,
    pub kind: Option<Kind>,
}

impl Filter {
    /// Restrict to one calendar month. Returns `false` if `month` is not 1..=12.
    pub fn set_month(&mut self, year: i32, month: u32) -> bool {
        let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
            return false;
        };
        let next_first = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
        };
        self.from = Some(first);
        self.to = next_first.and_then(|d| d.pred_opt());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_range_is_inclusive() {
        let mut f = Filter::default();
        assert!(f.set_month(2026, 10));
        assert_eq!(f.from, NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(f.to, NaiveDate::from_ymd_opt(2026, 10, 31));
    }

    #[test]
    fn december_and_leap_february() {
        let mut f = Filter::default();
        assert!(f.set_month(2026, 12));
        assert_eq!(f.to, NaiveDate::from_ymd_opt(2026, 12, 31));
        assert!(f.set_month(2024, 2));
        assert_eq!(f.to, NaiveDate::from_ymd_opt(2024, 2, 29));
    }

    #[test]
    fn invalid_month_rejected() {
        assert!(!Filter::default().set_month(2026, 13));
    }
}

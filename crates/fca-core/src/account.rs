//! Accounts and balances.
//!
//! An account is only a name on transactions; the optional `Account` record adds
//! an opening balance (the balance before the first recorded transaction).
//! `compute_balances` merges those with transaction sums. Must NOT do IO.

use std::collections::BTreeMap;

use crate::Money;

/// Settings for one account. Accounts without a record have opening balance 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub currency: String,
    pub opening_balance: Money,
}

/// An amount held in one account, in one currency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balance {
    pub account: String,
    pub currency: String,
    pub amount: Money,
}

/// Balance per (account, currency) = opening balance + sum of transactions.
///
/// `sums` holds the transaction totals per account and currency (already limited
/// to the date of interest). Account names match case-insensitively. Accounts
/// with a record but no transactions still appear, showing their opening balance.
/// The result is sorted by account name, then currency.
pub fn compute_balances(accounts: &[Account], sums: &[Balance]) -> Vec<Balance> {
    let mut map: BTreeMap<(String, String), Balance> = BTreeMap::new();
    let key = |account: &str, currency: &str| (account.to_lowercase(), currency.to_string());

    for sum in sums {
        let entry = map
            .entry(key(&sum.account, &sum.currency))
            .or_insert_with(|| Balance {
                account: sum.account.clone(),
                currency: sum.currency.clone(),
                amount: Money::default(),
            });
        entry.amount = entry.amount + sum.amount;
    }
    for account in accounts {
        let entry = map
            .entry(key(&account.name, &account.currency))
            .or_insert_with(|| Balance {
                account: account.name.clone(),
                currency: account.currency.clone(),
                amount: Money::default(),
            });
        entry.account = account.name.clone(); // the configured spelling wins
        entry.amount = entry.amount + account.opening_balance;
    }
    map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(account: &str, currency: &str, minor: i64) -> Balance {
        Balance {
            account: account.into(),
            currency: currency.into(),
            amount: Money::from_minor(minor),
        }
    }

    fn account(name: &str, currency: &str, opening: i64) -> Account {
        Account {
            name: name.into(),
            currency: currency.into(),
            opening_balance: Money::from_minor(opening),
        }
    }

    #[test]
    fn opening_balance_is_added_to_sums() {
        let got = compute_balances(
            &[account("Giro", "EUR", 100_000)],
            &[sum("giro", "EUR", -2_500)],
        );
        assert_eq!(got, [sum("Giro", "EUR", 97_500)]);
    }

    #[test]
    fn accounts_without_record_or_without_transactions_still_show() {
        let got = compute_balances(
            &[account("Savings", "EUR", 5_000)],
            &[sum("cash", "EUR", 300)],
        );
        assert_eq!(got, [sum("cash", "EUR", 300), sum("Savings", "EUR", 5_000)]);
    }

    #[test]
    fn currencies_are_kept_apart() {
        let got = compute_balances(
            &[account("giro", "EUR", 100)],
            &[sum("giro", "USD", 50), sum("giro", "EUR", 1)],
        );
        assert_eq!(got, [sum("giro", "EUR", 101), sum("giro", "USD", 50)]);
    }

    #[test]
    fn spelling_variants_are_merged() {
        let got = compute_balances(&[], &[sum("Cash", "EUR", 100), sum("cash", "EUR", 50)]);
        assert_eq!(got, [sum("Cash", "EUR", 150)]);
    }
}

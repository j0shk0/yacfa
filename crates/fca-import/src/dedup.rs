//! Step 3 of import: give every row a stable `import_hash`.
//!
//! The hash is `sha256(account|date|amount|description|n)` where `n` counts
//! identical rows seen earlier in the same file. Two real purchases of the same
//! coffee on one day therefore get different hashes and are both kept, while
//! importing the same file again produces the same hashes and adds nothing.

use std::collections::HashMap;

use fca_core::NewTransaction;
use sha2::{Digest, Sha256};

pub fn assign_hashes(transactions: &mut [NewTransaction]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for t in transactions {
        let key = format!(
            "{}|{}|{}|{}",
            t.account,
            t.date,
            t.amount.minor(),
            t.description
        );
        let n = seen.entry(key.clone()).or_insert(0);
        let digest = Sha256::digest(format!("{key}|{n}"));
        *n += 1;
        t.import_hash = Some(digest.iter().map(|byte| format!("{byte:02x}")).collect());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use fca_core::{Money, Origin};

    fn tx(day: u32, minor: i64, description: &str) -> NewTransaction {
        NewTransaction {
            date: NaiveDate::from_ymd_opt(2026, 10, day).unwrap(),
            amount: Money::from_minor(minor),
            currency: "EUR".into(),
            description: description.into(),
            account: "giro".into(),
            payee: None,
            origin: Origin::Import,
            recurring_id: None,
            import_hash: None,
            labels: vec![],
        }
    }

    fn hashes(mut rows: Vec<NewTransaction>) -> Vec<String> {
        assign_hashes(&mut rows);
        rows.into_iter().map(|t| t.import_hash.unwrap()).collect()
    }

    #[test]
    fn identical_rows_in_one_file_get_different_hashes() {
        let h = hashes(vec![
            tx(1, -350, "Coffee"),
            tx(1, -350, "Coffee"),
            tx(2, -350, "Coffee"),
        ]);
        assert_eq!(h.len(), 3);
        assert!(h[0] != h[1] && h[1] != h[2] && h[0] != h[2]);
    }

    #[test]
    fn hashes_are_stable_across_runs() {
        let rows = || vec![tx(1, -350, "Coffee"), tx(1, -350, "Coffee")];
        assert_eq!(hashes(rows()), hashes(rows()));
        assert_eq!(hashes(rows())[0].len(), 64);
    }
}

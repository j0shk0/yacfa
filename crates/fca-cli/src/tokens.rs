//! Taskwarrior-style token parsing: `+label`, `-label`, `key:value`, plain words.
//!
//! Splits raw strings into those groups and parses dates. It does not know which
//! keys a command accepts; commands `take` the keys they know and call `finish`,
//! which rejects anything left over. Must NOT touch the store or print.

use anyhow::{anyhow, bail, Result};
use chrono::{Days, NaiveDate};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    /// Everything that is not a label or attribute, in order.
    pub words: Vec<String>,
    pub add_labels: Vec<String>,
    pub remove_labels: Vec<String>,
    attrs: Vec<(String, String)>,
}

impl Tokens {
    /// Split `args` into words, labels and attributes.
    ///
    /// - `+food` / `-food`: a label (the character after the sign must be a letter,
    ///   so `-12,50` stays a word).
    /// - `account:cash`: an attribute; the key must be lowercase ASCII letters, so
    ///   `Re: invoice` and `12:30` stay words, as do URLs (`https://...`). The value
    ///   may be empty (`payee:`).
    pub fn parse(args: &[String]) -> Tokens {
        let mut t = Tokens::default();
        for arg in args {
            if let Some(name) = label_name(arg, '+') {
                t.add_labels.push(name.to_string());
            } else if let Some(name) = label_name(arg, '-') {
                t.remove_labels.push(name.to_string());
            } else if let Some((key, value)) = attr(arg) {
                t.attrs.push((key.to_string(), value.to_string()));
            } else {
                t.words.push(arg.clone());
            }
        }
        t
    }

    /// Remove and return the value of attribute `key` (the last one if repeated).
    pub fn take(&mut self, key: &str) -> Option<String> {
        let value = self
            .attrs
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone());
        self.attrs.retain(|(k, _)| k != key);
        value
    }

    /// Error if any attribute was not taken, so typos like `acount:cash` are caught.
    pub fn finish(&self) -> Result<()> {
        match self.attrs.first() {
            Some((key, _)) => bail!("unknown attribute `{key}:`"),
            None => Ok(()),
        }
    }
}

fn label_name(arg: &str, sign: char) -> Option<&str> {
    let rest = arg.strip_prefix(sign)?;
    rest.starts_with(|c: char| c.is_alphabetic())
        .then_some(rest)
}

fn attr(arg: &str) -> Option<(&str, &str)> {
    let (key, value) = arg.split_once(':')?;
    let is_key = !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase());
    // `https://...` is a URL inside a description, not an attribute.
    (is_key && !value.starts_with("//")).then_some((key, value))
}

/// `YYYY-MM-DD`, or `today` / `yesterday` / `tomorrow`.
pub fn parse_date(s: &str, today: NaiveDate) -> Result<NaiveDate> {
    match s {
        "today" => Ok(today),
        "yesterday" => today
            .checked_sub_days(Days::new(1))
            .ok_or_else(|| anyhow!("date out of range")),
        "tomorrow" => today
            .checked_add_days(Days::new(1))
            .ok_or_else(|| anyhow!("date out of range")),
        _ => NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
            anyhow!("invalid date `{s}` (expected YYYY-MM-DD, today, yesterday or tomorrow)")
        }),
    }
}

/// `YYYY-MM` into (year, month).
pub fn parse_month(s: &str) -> Result<(i32, u32)> {
    let bad = || anyhow!("invalid month `{s}` (expected YYYY-MM)");
    let (year, month) = s.split_once('-').ok_or_else(bad)?;
    Ok((
        year.parse().map_err(|_| bad())?,
        month.parse().map_err(|_| bad())?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    #[test]
    fn splits_words_labels_and_attrs() {
        let mut t = Tokens::parse(&args(
            "12,50 Groceries +food -old account:cash payee:Rewe date:2026-11-01",
        ));
        assert_eq!(t.words, ["12,50", "Groceries"]);
        assert_eq!(t.add_labels, ["food"]);
        assert_eq!(t.remove_labels, ["old"]);
        assert_eq!(t.take("account").as_deref(), Some("cash"));
        assert_eq!(t.take("payee").as_deref(), Some("Rewe"));
        assert_eq!(t.take("date").as_deref(), Some("2026-11-01"));
        assert!(t.finish().is_ok());
    }

    #[test]
    fn negative_numbers_and_odd_words_stay_words() {
        let t = Tokens::parse(&args("-12,50 +5 Re: 12:30 http://x -"));
        assert_eq!(t.words, ["-12,50", "+5", "Re:", "12:30", "http://x", "-"]);
        assert!(t.add_labels.is_empty() && t.remove_labels.is_empty());
        assert!(t.finish().is_ok());
    }

    #[test]
    fn empty_value_and_repeated_keys() {
        let mut t = Tokens::parse(&args("payee: account:a account:b"));
        assert_eq!(t.take("payee").as_deref(), Some(""));
        assert_eq!(t.take("account").as_deref(), Some("b"));
        assert_eq!(t.take("account"), None);
    }

    #[test]
    fn leftover_attribute_is_an_error() {
        let t = Tokens::parse(&args("acount:cash"));
        assert!(t.finish().unwrap_err().to_string().contains("acount"));
    }

    #[test]
    fn parses_dates_and_months() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        assert_eq!(
            parse_date("2026-10-01", today).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
        );
        assert_eq!(
            parse_date("yesterday", today).unwrap(),
            NaiveDate::from_ymd_opt(2026, 11, 8).unwrap()
        );
        assert!(parse_date("01.10.2026", today).is_err());
        assert_eq!(parse_month("2026-10").unwrap(), (2026, 10));
        assert!(parse_month("2026").is_err());
    }
}

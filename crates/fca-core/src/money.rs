//! `Money`: a signed amount stored as whole minor units (cents), never as a float.
//!
//! Negative means expense, positive means income. Must NOT know about currencies;
//! the currency travels next to the amount in `Transaction`.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, Neg};

use thiserror::Error;

/// A signed amount in minor units (1/100 of the currency unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(i64);

/// Why an amount string could not be parsed.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid amount {input:?}: {reason}")]
pub struct ParseMoneyError {
    input: String,
    reason: &'static str,
}

impl Money {
    pub const fn from_minor(minor: i64) -> Self {
        Money(minor)
    }

    pub const fn minor(self) -> i64 {
        self.0
    }

    /// Parse with explicit separators, e.g. `parse("1.234,56", ',', '.')` is 123456.
    ///
    /// Accepts an optional leading `+`/`-`, at most 2 decimal digits, and
    /// thousands separators only in the integer part, in groups of exactly 3.
    pub fn parse(input: &str, decimal: char, thousands: char) -> Result<Money, ParseMoneyError> {
        let fail = |reason| ParseMoneyError {
            input: input.to_string(),
            reason,
        };
        let s = input.trim();
        let (negative, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };

        let (int_raw, frac) = match body.split_once(decimal) {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };
        if frac.contains(decimal) {
            return Err(fail("more than one decimal separator"));
        }
        if frac.len() > 2 {
            return Err(fail("more than 2 decimal digits"));
        }
        if !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(fail("unexpected character"));
        }
        let int_digits =
            strip_thousands(int_raw, thousands).ok_or_else(|| fail("bad thousands grouping"))?;
        if int_digits.is_empty() && frac.is_empty() {
            return Err(fail("no digits"));
        }

        let major: i64 = if int_digits.is_empty() {
            0
        } else {
            int_digits
                .parse()
                .map_err(|_| fail("not a number or too large"))?
        };
        let frac_minor: i64 = format!("{frac:0<2}")
            .parse()
            .map_err(|_| fail("bad decimals"))?;
        let minor = major
            .checked_mul(100)
            .and_then(|m| m.checked_add(frac_minor))
            .ok_or_else(|| fail("too large"))?;
        Ok(Money(if negative { -minor } else { minor }))
    }

    /// Parse user-typed amounts without knowing the locale: `12,50`, `12.50`,
    /// `1.234,56`, `1,234.56` and `1,234` (thousands) all work.
    ///
    /// If one separator appears once with exactly 3 digits after it and no other
    /// separator is present, it is read as thousands, because 3 decimals are invalid anyway.
    pub fn parse_auto(input: &str) -> Result<Money, ParseMoneyError> {
        let s = input.trim();
        let Some(idx) = s.rfind([',', '.']) else {
            return Money::parse(s, '.', ',');
        };
        let last = if s[idx..].starts_with(',') { ',' } else { '.' };
        let other = if last == ',' { '.' } else { ',' };
        let digits_after = s.len() - idx - 1;
        let last_is_thousands =
            s.matches(last).count() > 1 || (!s.contains(other) && digits_after == 3);
        if last_is_thousands {
            Money::parse(s, other, last)
        } else {
            Money::parse(s, last, other)
        }
    }
}

/// Remove thousands separators if every group after the first has exactly 3 digits.
fn strip_thousands(int_raw: &str, thousands: char) -> Option<String> {
    let groups: Vec<&str> = int_raw.split(thousands).collect();
    if groups.len() > 1
        && (groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3))
    {
        return None;
    }
    let joined = groups.concat();
    joined.chars().all(|c| c.is_ascii_digit()).then_some(joined)
}

impl fmt::Display for Money {
    /// Always `-12.50` style with a dot, so output is stable and machine-friendly.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:02}", abs / 100, abs % 100)
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money(0), |a, b| a + b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eu(s: &str) -> Result<Money, ParseMoneyError> {
        Money::parse(s, ',', '.')
    }

    #[test]
    fn parses_eu_format_with_thousands() {
        assert_eq!(eu("1.234,56").unwrap().minor(), 123456);
        assert_eq!(eu("12,5").unwrap().minor(), 1250);
        assert_eq!(eu("7").unwrap().minor(), 700);
        assert_eq!(eu(",50").unwrap().minor(), 50);
    }

    #[test]
    fn parses_negative_and_plus() {
        assert_eq!(eu("-1.234,56").unwrap().minor(), -123456);
        assert_eq!(eu("+3,00").unwrap().minor(), 300);
    }

    #[test]
    fn rejects_garbage() {
        assert!(eu("12,3,4").is_err());
        assert!(eu("12,345").is_err());
        assert!(eu("1.2.3,4").is_err());
        assert!(eu("12a").is_err());
        assert!(eu("").is_err());
        assert!(eu("-").is_err());
        assert!(eu("99999999999999999999").is_err());
    }

    #[test]
    fn auto_detects_separators() {
        let cases = [
            ("12,50", 1250),
            ("12.50", 1250),
            ("1.234,56", 123456),
            ("1,234.56", 123456),
            ("1.234", 123400),
            ("2500", 250000),
            ("-0,99", -99),
        ];
        for (input, minor) in cases {
            assert_eq!(Money::parse_auto(input).unwrap().minor(), minor, "{input}");
        }
        assert!(Money::parse_auto("12,3,4").is_err());
    }

    #[test]
    fn displays_with_dot_and_sign() {
        assert_eq!(Money::from_minor(123456).to_string(), "1234.56");
        assert_eq!(Money::from_minor(-5).to_string(), "-0.05");
        assert_eq!(Money::from_minor(0).to_string(), "0.00");
    }

    #[test]
    fn sums() {
        let total: Money = [100, -250, 5].into_iter().map(Money::from_minor).sum();
        assert_eq!(total.minor(), -145);
    }
}

//! `Schedule`: how often a recurring entry repeats, and which dates that produces.
//!
//! The k-th occurrence is computed as `start + k*n units`, never by stepping from
//! the previous date, so a monthly schedule starting Jan 31 stays at month-end.
//! Must NOT know about transactions or the database.

use std::fmt;
use std::str::FromStr;

use chrono::{Days, Months, NaiveDate};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

/// A repeat rule. More variants (e.g. an RFC 5545 RRULE) can be added later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    /// Every `n` units, `n >= 1`.
    Every { n: u32, unit: Unit },
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid schedule {0:?} (try: monthly, every 2 weeks, 10d, 3m)")]
pub struct ParseScheduleError(String);

impl Schedule {
    /// The `k`-th occurrence (0 is `start` itself), or `None` on date overflow.
    pub fn occurrence(&self, start: NaiveDate, k: u32) -> Option<NaiveDate> {
        let Schedule::Every { n, unit } = *self;
        let steps = n.checked_mul(k)?;
        match unit {
            Unit::Day => start.checked_add_days(Days::new(u64::from(steps))),
            Unit::Week => start.checked_add_days(Days::new(u64::from(steps) * 7)),
            Unit::Month => start.checked_add_months(Months::new(steps)),
            Unit::Year => start.checked_add_months(Months::new(steps.checked_mul(12)?)),
        }
    }

    /// All occurrences from `start` up to and including `until`, in order.
    pub fn occurrences_until(&self, start: NaiveDate, until: NaiveDate) -> Vec<NaiveDate> {
        (0..)
            .map_while(|k| self.occurrence(start, k))
            .take_while(|d| *d <= until)
            .collect()
    }
}

impl FromStr for Schedule {
    type Err = ParseScheduleError;

    /// Accepts `daily|weekly|monthly|quarterly|yearly`, `every 2 weeks`, `every month`, `2w`, `10d`, `3m`, `1y`.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let fail = || ParseScheduleError(input.to_string());
        let s = input.trim().to_lowercase();
        let every = |n, unit| Ok(Schedule::Every { n, unit });
        match s.as_str() {
            "daily" => return every(1, Unit::Day),
            "weekly" => return every(1, Unit::Week),
            "monthly" => return every(1, Unit::Month),
            "quarterly" => return every(3, Unit::Month),
            "yearly" => return every(1, Unit::Year),
            _ => {}
        }

        // Long form "every [N] unit" or short form "<N><d|w|m|y>".
        let (count, unit_word) = if let Some(rest) = s.strip_prefix("every ") {
            match rest.trim().split_once(char::is_whitespace) {
                Some((n, word)) => (n, word.trim()),
                None => ("1", rest.trim()),
            }
        } else {
            let split = s.find(|c: char| !c.is_ascii_digit()).ok_or_else(fail)?;
            s.split_at(split)
        };
        let n: u32 = count.parse().map_err(|_| fail())?;
        if n == 0 {
            return Err(fail());
        }
        let unit = match unit_word {
            "d" | "day" | "days" => Unit::Day,
            "w" | "week" | "weeks" => Unit::Week,
            "m" | "month" | "months" => Unit::Month,
            "y" | "year" | "years" => Unit::Year,
            _ => return Err(fail()),
        };
        every(n, unit)
    }
}

impl fmt::Display for Schedule {
    /// Canonical short form (`2w`); it parses back to the same value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Schedule::Every { n, unit } = *self;
        let c = match unit {
            Unit::Day => 'd',
            Unit::Week => 'w',
            Unit::Month => 'm',
            Unit::Year => 'y',
        };
        write!(f, "{n}{c}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn sched(s: &str) -> Schedule {
        s.parse().unwrap()
    }

    #[test]
    fn parses_all_forms() {
        let two_weeks = Schedule::Every {
            n: 2,
            unit: Unit::Week,
        };
        assert_eq!(sched("every 2 weeks"), two_weeks);
        assert_eq!(sched("2w"), two_weeks);
        assert_eq!(sched("Every 2 Weeks"), two_weeks);
        assert_eq!(
            sched("monthly"),
            Schedule::Every {
                n: 1,
                unit: Unit::Month
            }
        );
        assert_eq!(
            sched("every month"),
            Schedule::Every {
                n: 1,
                unit: Unit::Month
            }
        );
        assert_eq!(
            sched("quarterly"),
            Schedule::Every {
                n: 3,
                unit: Unit::Month
            }
        );
        assert_eq!(
            sched("10d"),
            Schedule::Every {
                n: 10,
                unit: Unit::Day
            }
        );
        assert_eq!(
            sched("1y"),
            Schedule::Every {
                n: 1,
                unit: Unit::Year
            }
        );
    }

    #[test]
    fn rejects_bad_input() {
        for bad in ["", "often", "0w", "w", "every", "every 2", "2x", "-1d"] {
            assert!(bad.parse::<Schedule>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn display_round_trips() {
        for s in ["3m", "10d", "2w", "1y"] {
            assert_eq!(sched(s).to_string(), s);
        }
    }

    #[test]
    fn monthly_from_jan_31_stays_at_month_end() {
        let got = sched("monthly").occurrences_until(d(2026, 1, 31), d(2026, 5, 31));
        assert_eq!(
            got,
            [
                d(2026, 1, 31),
                d(2026, 2, 28),
                d(2026, 3, 31),
                d(2026, 4, 30),
                d(2026, 5, 31)
            ]
        );
    }

    #[test]
    fn weekly_includes_start_and_until() {
        let got = sched("weekly").occurrences_until(d(2026, 10, 1), d(2026, 10, 22));
        assert_eq!(got.len(), 4);
        assert_eq!(got[3], d(2026, 10, 22));
    }

    #[test]
    fn nothing_before_start() {
        assert!(sched("daily")
            .occurrences_until(d(2026, 10, 5), d(2026, 10, 1))
            .is_empty());
    }

    #[test]
    fn leap_day_yearly() {
        assert_eq!(
            sched("yearly").occurrence(d(2024, 2, 29), 1),
            Some(d(2025, 2, 28))
        );
        assert_eq!(
            sched("yearly").occurrence(d(2024, 2, 29), 4),
            Some(d(2028, 2, 29))
        );
    }
}

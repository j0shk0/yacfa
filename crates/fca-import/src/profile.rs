//! `ImportProfile`: how to read one bank's CSV export, loaded from the config file.
//!
//! Plain serde data plus a `validate` check. Must NOT read files or parse CSV.

use std::collections::HashMap;

use serde::Deserialize;

use crate::ImportError;

/// The `[import]` section of the config file.
#[derive(Debug, Default, Deserialize)]
pub struct ImportConfig {
    /// Keyed by profile name: `[import.profiles.mybank]`.
    #[serde(default)]
    pub profiles: HashMap<String, ImportProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportProfile {
    /// A single ASCII character.
    #[serde(default = "default_delimiter")]
    pub delimiter: char,
    /// Any label `encoding_rs` knows, e.g. `windows-1252`.
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// Lines to drop before the header row (bank preambles).
    #[serde(default)]
    pub skip_rows: usize,
    /// chrono format, e.g. `%d.%m.%Y`.
    pub date_format: String,
    #[serde(default = "default_decimal")]
    pub decimal_separator: char,
    /// Omit if amounts have no thousands grouping.
    pub thousands_separator: Option<char>,
    /// Account all rows are booked to.
    pub account: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    /// Labels added to every imported row.
    #[serde(default)]
    pub labels: Vec<String>,
    pub columns: Columns,
}

/// Header names of the CSV columns to read.
#[derive(Debug, Clone, Deserialize)]
pub struct Columns {
    pub date: String,
    /// A single signed amount column...
    pub amount: Option<String>,
    /// ...or separate columns for money out and money in (both read as positive).
    pub debit: Option<String>,
    pub credit: Option<String>,
    pub payee: Option<String>,
    /// One or more columns, joined with `" / "`.
    #[serde(default)]
    pub description: Vec<String>,
}

fn default_delimiter() -> char {
    ','
}
fn default_encoding() -> String {
    "utf-8".to_string()
}
fn default_decimal() -> char {
    '.'
}
fn default_currency() -> String {
    fca_core::DEFAULT_CURRENCY.to_string()
}

impl ImportProfile {
    /// Reject profiles that can never work, before touching the file.
    pub fn validate(&self) -> Result<(), ImportError> {
        let bad = |msg: &str| Err(ImportError::Profile(msg.to_string()));
        if !self.delimiter.is_ascii() {
            return bad("delimiter must be one ASCII character");
        }
        let c = &self.columns;
        if c.amount.is_none() && c.debit.is_none() && c.credit.is_none() {
            return bad("columns need `amount`, or `debit` and/or `credit`");
        }
        if c.amount.is_some() && (c.debit.is_some() || c.credit.is_some()) {
            return bad("use either `amount` or `debit`/`credit`, not both");
        }
        if self.thousands_separator == Some(self.decimal_separator) {
            return bad("decimal and thousands separators must differ");
        }
        Ok(())
    }

    /// The thousands separator as a `char`; NUL (never in real data) means none.
    pub(crate) fn thousands(&self) -> char {
        self.thousands_separator.unwrap_or('\0')
    }
}

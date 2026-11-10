//! Bank CSV import: turns CSV bytes plus an `ImportProfile` into `NewTransaction`s.
//!
//! Pipeline: `decode` (bytes to text) -> `parse` (rows to transactions) ->
//! `dedup` (stable `import_hash` per row). Must NOT touch the database;
//! the CLI hands the result to the store, which skips already-imported rows.

mod decode;
mod dedup;
mod parse;
mod profile;

use fca_core::NewTransaction;
use thiserror::Error;

pub use profile::{Columns, ImportConfig, ImportProfile};

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("invalid import profile: {0}")]
    Profile(String),
    #[error("unknown encoding {0:?}")]
    UnknownEncoding(String),
    #[error("file is not valid {0}")]
    Decode(&'static str),
    #[error("column {name:?} not found in the header (found: {found})")]
    MissingColumn { name: String, found: String },
    #[error("line {line}: {message}")]
    Row { line: u64, message: String },
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
}

/// Read a bank export. Every returned transaction has `origin = Import`
/// and an `import_hash` that is stable across runs.
pub fn import_csv(
    bytes: &[u8],
    profile: &ImportProfile,
) -> Result<Vec<NewTransaction>, ImportError> {
    profile.validate()?;
    let text = decode::decode(bytes, &profile.encoding, profile.skip_rows)?;
    let mut transactions = parse::parse_records(&text, profile)?;
    dedup::assign_hashes(&mut transactions);
    Ok(transactions)
}

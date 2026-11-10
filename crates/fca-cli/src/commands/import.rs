//! `fca import <file> --profile <name> [--dry-run]`.
//!
//! Reads the file, lets `fca-import` turn it into transactions, and hands them to
//! the store, which skips rows imported before. Must NOT parse CSV itself.

use std::path::Path;

use anyhow::{anyhow, Context as _, Result};
use fca_store::Store;

use super::{Context, Output};

pub async fn run(
    ctx: &Context<'_>,
    file: &Path,
    profile_name: &str,
    dry_run: bool,
) -> Result<Output> {
    let profiles = &ctx.config.import.profiles;
    let profile = profiles.get(profile_name).ok_or_else(|| {
        let mut names: Vec<&str> = profiles.keys().map(String::as_str).collect();
        names.sort();
        anyhow!(
            "no import profile `{profile_name}` in the config (available: {})",
            names.join(", ")
        )
    })?;

    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let transactions = fca_import::import_csv(&bytes, profile)
        .with_context(|| format!("importing {}", file.display()))?;

    if dry_run {
        return Ok(Output::ImportPreview(transactions));
    }
    let summary = ctx.store.insert_transactions(&transactions).await?;
    Ok(Output::Imported {
        inserted: summary.inserted,
        skipped: summary.skipped,
    })
}

//! `fca expense` / `fca income`: create one manual transaction from tokens.
//!
//! Also provides `parse_entry`, the shared reading of "amount description +labels
//! account: payee: currency:" that `recur` reuses. Must NOT print.

use anyhow::{bail, Context as _, Result};
use fca_core::{Money, NewTransaction, Origin};
use fca_store::Store;

use super::{Context, Output};
use crate::tokens::Tokens;

#[derive(Clone, Copy)]
pub enum Direction {
    Expense,
    Income,
}

/// Fields shared by one-off and recurring entries.
pub struct Entry {
    /// Signed: negative for expenses.
    pub amount: Money,
    pub description: String,
    pub account: String,
    pub currency: String,
    pub payee: Option<String>,
    pub labels: Vec<String>,
}

/// Read the shared fields; the caller then takes its own keys and calls `finish`.
pub fn parse_entry(ctx: &Context<'_>, tokens: &mut Tokens, direction: Direction) -> Result<Entry> {
    if !tokens.remove_labels.is_empty() {
        bail!("`-label` only makes sense with `modify`");
    }
    let mut words = tokens.words.iter();
    let amount_text = words
        .next()
        .context("missing amount, e.g. `fca expense 12,50 Groceries`")?;
    let magnitude = Money::parse_auto(amount_text)?.minor().abs();
    let amount = match direction {
        Direction::Expense => Money::from_minor(-magnitude),
        Direction::Income => Money::from_minor(magnitude),
    };
    let description = words.cloned().collect::<Vec<_>>().join(" ");
    if description.is_empty() {
        bail!("missing description, e.g. `fca expense 12,50 Groceries`");
    }
    Ok(Entry {
        amount,
        description,
        account: tokens
            .take("account")
            .unwrap_or_else(|| ctx.config.account().to_string()),
        currency: tokens
            .take("currency")
            .unwrap_or_else(|| ctx.config.currency().to_string()),
        payee: tokens.take("payee").filter(|p| !p.is_empty()),
        labels: tokens.add_labels.clone(),
    })
}

pub async fn run(ctx: &Context<'_>, direction: Direction, args: &[String]) -> Result<Output> {
    let mut tokens = Tokens::parse(args);
    let entry = parse_entry(ctx, &mut tokens, direction)?;
    let date = match tokens.take("date") {
        Some(text) => crate::tokens::parse_date(&text, ctx.today)?,
        None => ctx.today,
    };
    tokens.finish()?;

    let new = NewTransaction {
        date,
        amount: entry.amount,
        currency: entry.currency.clone(),
        description: entry.description.clone(),
        account: entry.account,
        payee: entry.payee,
        origin: Origin::Manual,
        recurring_id: None,
        import_hash: None,
        labels: entry.labels,
    };
    let id = ctx
        .store
        .insert_transaction(&new)
        .await?
        .context("transaction was not inserted")?;
    Ok(Output::Created {
        id,
        amount: entry.amount,
        currency: entry.currency,
        description: entry.description,
    })
}

//! `fca transfer <amount> from:<account> to:<account> [description] [date:D] [+label]`.
//!
//! Reads the tokens into a `NewTransfer`; `fca-core` validates it and the store
//! saves both sides. Must NOT print.

use anyhow::{bail, Context as _, Result};
use fca_core::{Money, NewTransfer};
use fca_store::Store;

use super::{Context, Output};
use crate::tokens::{parse_date, Tokens};

pub async fn run(ctx: &Context<'_>, args: &[String]) -> Result<Output> {
    let mut tokens = Tokens::parse(args);
    if !tokens.remove_labels.is_empty() {
        bail!("`-label` only makes sense with `modify`");
    }
    let mut words = tokens.words.iter();
    let amount_text = words
        .next()
        .context("missing amount, e.g. `fca transfer 200 from:giro to:savings`")?;
    let amount = Money::from_minor(Money::parse_auto(amount_text)?.minor().abs());
    let description = words.cloned().collect::<Vec<_>>().join(" ");

    let from = tokens.take("from").context("missing `from:<account>`")?;
    let to = tokens.take("to").context("missing `to:<account>`")?;
    let date = match tokens.take("date") {
        Some(text) => parse_date(&text, ctx.today)?,
        None => ctx.today,
    };
    let currency = tokens
        .take("currency")
        .unwrap_or_else(|| ctx.config.currency().to_string());
    tokens.finish()?;

    let transfer = NewTransfer {
        date,
        amount,
        currency: currency.clone(),
        from: from.clone(),
        to: to.clone(),
        description: Some(description).filter(|d| !d.is_empty()),
        labels: tokens.add_labels.clone(),
    };
    let (out_id, in_id) = ctx.store.add_transfer(&transfer).await?;
    Ok(Output::TransferCreated {
        out_id,
        in_id,
        amount,
        currency,
        from,
        to,
    })
}

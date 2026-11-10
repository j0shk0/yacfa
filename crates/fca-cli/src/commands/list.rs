//! `fca list`: turn filter tokens into a `Filter` and query the store.
//!
//! Must NOT format output (that is `output.rs`).

use anyhow::{anyhow, bail, Result};
use fca_core::{Filter, Kind, Origin};
use fca_store::Store;

use super::{Context, Output};
use crate::tokens::{parse_date, parse_month, Tokens};

pub async fn run(ctx: &Context<'_>, args: &[String]) -> Result<Output> {
    let filter = parse_filter(ctx, args)?;
    Ok(Output::Transactions(
        ctx.store.list_transactions(&filter).await?,
    ))
}

fn parse_filter(ctx: &Context<'_>, args: &[String]) -> Result<Filter> {
    let mut tokens = Tokens::parse(args);
    if let Some(word) = tokens.words.first() {
        bail!("unexpected `{word}` (filters look like `+label` or `account:cash`)");
    }
    if let Some(label) = tokens.remove_labels.first() {
        bail!("`-{label}` is not supported in list filters");
    }
    let mut filter = Filter {
        labels: tokens.add_labels.clone(),
        ..Filter::default()
    };
    filter.account = tokens.take("account");
    filter.payee = tokens.take("payee");
    if let Some(text) = tokens.take("from") {
        filter.from = Some(parse_date(&text, ctx.today)?);
    }
    if let Some(text) = tokens.take("to") {
        filter.to = Some(parse_date(&text, ctx.today)?);
    }
    if let Some(text) = tokens.take("month") {
        let (year, month) = parse_month(&text)?;
        if !filter.set_month(year, month) {
            bail!("invalid month `{text}`");
        }
    }
    if let Some(text) = tokens.take("origin") {
        filter.origin = Some(text.parse::<Origin>()?);
    }
    if let Some(text) = tokens.take("type") {
        filter.kind = Some(match text.as_str() {
            "income" => Kind::Income,
            "expense" => Kind::Expense,
            "transfer" => Kind::Transfer,
            _ => {
                return Err(anyhow!(
                    "invalid type `{text}` (expected income, expense or transfer)"
                ))
            }
        });
    }
    tokens.finish()?;
    Ok(filter)
}

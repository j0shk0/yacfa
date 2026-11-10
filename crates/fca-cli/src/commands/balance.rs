//! `fca balance [asof:D]`: balance of every account on a date (default today).
//! A balance is the opening balance plus all transactions up to and including that date.

use anyhow::{bail, Result};
use fca_store::Store;

use super::{Context, Output};
use crate::tokens::{parse_date, Tokens};

pub async fn run(ctx: &Context<'_>, args: &[String]) -> Result<Output> {
    let mut tokens = Tokens::parse(args);
    if let Some(word) = tokens.words.first() {
        bail!("unexpected `{word}` (the only option is `asof:<date>`)");
    }
    let asof = match tokens.take("asof") {
        Some(text) => parse_date(&text, ctx.today)?,
        None => ctx.today,
    };
    tokens.finish()?;
    Ok(Output::Balances {
        asof,
        balances: ctx.store.balances(asof).await?,
    })
}

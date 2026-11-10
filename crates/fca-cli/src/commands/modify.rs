//! `fca modify <id> ...`: turn tokens into a `TransactionPatch` and apply it.
//!
//! `amount:` is taken literally (negative = expense). Must NOT print.

use anyhow::{bail, Result};
use fca_core::{Money, TransactionPatch};
use fca_store::Store;

use super::{Context, Output};
use crate::tokens::{parse_date, Tokens};

pub async fn run(ctx: &Context<'_>, id: i64, args: &[String]) -> Result<Output> {
    let patch = parse_patch(ctx, args)?;
    Ok(Output::Modified(
        ctx.store.modify_transaction(id, &patch).await?,
    ))
}

fn parse_patch(ctx: &Context<'_>, args: &[String]) -> Result<TransactionPatch> {
    let mut tokens = Tokens::parse(args);
    if let Some(word) = tokens.words.first() {
        bail!("unexpected `{word}` (use `desc:...` to change the description)");
    }
    let mut patch = TransactionPatch {
        add_labels: tokens.add_labels.clone(),
        remove_labels: tokens.remove_labels.clone(),
        ..TransactionPatch::default()
    };
    patch.description = tokens.take("desc");
    if let Some(text) = tokens.take("amount") {
        patch.amount = Some(Money::parse_auto(&text)?);
    }
    if let Some(text) = tokens.take("date") {
        patch.date = Some(parse_date(&text, ctx.today)?);
    }
    patch.account = tokens.take("account");
    // `payee:` with an empty value clears the payee.
    patch.payee = tokens
        .take("payee")
        .map(|p| Some(p).filter(|p| !p.is_empty()));
    tokens.finish()?;
    if patch == TransactionPatch::default() {
        bail!("nothing to modify");
    }
    Ok(patch)
}

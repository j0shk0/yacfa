//! `fca link <a> <b>` and `fca unlink <id>`: pair up existing transactions as a transfer.
//!
//! Useful after importing two bank exports, where one payment shows up as an
//! expense in one account and income in the other. The store checks the rules.

use anyhow::Result;
use fca_store::Store;

use super::{Context, Output};

pub async fn link(ctx: &Context<'_>, a: i64, b: i64) -> Result<Output> {
    ctx.store.link_transfers(a, b).await?;
    Ok(Output::Linked { a, b })
}

pub async fn unlink(ctx: &Context<'_>, id: i64) -> Result<Output> {
    ctx.store.unlink_transfer(id).await?;
    Ok(Output::Unlinked(id))
}

//! `fca delete <id>`: remove one transaction.

use anyhow::Result;
use fca_store::Store;

use super::{Context, Output};

pub async fn run(ctx: &Context<'_>, id: i64) -> Result<Output> {
    Ok(Output::Deleted(ctx.store.delete_transaction(id).await?))
}

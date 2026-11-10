//! `fca labels`: labels in use with their transaction counts.
//! Labels are created implicitly when first used, so there is no "add label".

use anyhow::Result;
use fca_store::Store;

use super::{Context, Output};

pub async fn run(ctx: &Context<'_>) -> Result<Output> {
    Ok(Output::Labels(ctx.store.list_labels().await?))
}

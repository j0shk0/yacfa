//! Entry point of the `fca` binary. Only this crate may print to the terminal.
//!
//! Stays thin: parse arguments, load config, open the store, run one command,
//! print what it returned. All real work happens in `commands/`.

mod cli;
mod commands;
mod config;
mod output;
mod tokens;

use anyhow::{Context as _, Result};
use clap::Parser;
use fca_store::{SqliteStore, Store};

use cli::{Cli, Command, RecurCommand};
use commands::Context;
use config::Config;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load(cli.config.as_deref())?;
    let target = config::resolve_db_target(cli.db.as_deref(), &config);
    config::prepare_db_target(&target)?;
    let store = SqliteStore::connect(&target)
        .await
        .with_context(|| format!("opening database {target}"))?;

    let today = chrono::Local::now().date_naive();
    let ctx = Context {
        store: &store,
        config: &config,
        today,
    };
    // Like Taskwarrior, due recurring transactions are generated lazily before
    // every command. `recur sync` does it itself so it can report the count.
    if !matches!(cli.command, Command::Recur(RecurCommand::Sync)) {
        store.sync_recurring(today).await?;
    }
    let result = commands::run(cli.command, &ctx).await?;
    print!("{}", output::render(&result));
    Ok(())
}

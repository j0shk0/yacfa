//! Command dispatch. Each command is a function that returns an `Output`
//! (data), never prints; `output.rs` renders it.
//!
//! To add a command: a variant in `cli.rs`, a file here, and an arm in `run`.

mod account;
mod add;
mod balance;
mod delete;
mod import;
mod labels;
mod link;
mod list;
mod modify;
mod recur;
mod transfer;

use anyhow::Result;
use chrono::NaiveDate;
use fca_core::{Account, Balance, Money, NewTransaction, Recurring, Transaction};
use fca_store::{LabelCount, SqliteStore};

use crate::cli::{AccountCommand, Command};
use crate::config::Config;

/// Everything a command needs. `today` is injected so logic stays testable.
pub struct Context<'a> {
    pub store: &'a SqliteStore,
    pub config: &'a Config,
    pub today: NaiveDate,
}

/// What a command produced.
pub enum Output {
    Created {
        id: i64,
        amount: Money,
        currency: String,
        description: String,
    },
    Modified(Transaction),
    /// Ids removed; two ids means both sides of a transfer.
    Deleted(Vec<i64>),
    TransferCreated {
        out_id: i64,
        in_id: i64,
        amount: Money,
        currency: String,
        from: String,
        to: String,
    },
    Linked {
        a: i64,
        b: i64,
    },
    Unlinked(i64),
    Balances {
        asof: NaiveDate,
        balances: Vec<Balance>,
    },
    AccountSet(Account),
    Transactions(Vec<Transaction>),
    Labels(Vec<LabelCount>),
    /// Rows read from a CSV, not saved (`--dry-run`).
    ImportPreview(Vec<NewTransaction>),
    Imported {
        inserted: usize,
        skipped: usize,
    },
    RecurringAdded {
        id: i64,
        description: String,
        schedule: String,
        generated: usize,
    },
    RecurringList(Vec<Recurring>),
    RecurringDeleted(i64),
    Synced(usize),
}

pub async fn run(command: Command, ctx: &Context<'_>) -> Result<Output> {
    match command {
        Command::Expense(args) => add::run(ctx, add::Direction::Expense, &args.tokens).await,
        Command::Income(args) => add::run(ctx, add::Direction::Income, &args.tokens).await,
        Command::List(args) => list::run(ctx, &args.tokens).await,
        Command::Modify { id, args } => modify::run(ctx, id, &args.tokens).await,
        Command::Delete { id } => delete::run(ctx, id).await,
        Command::Labels => labels::run(ctx).await,
        Command::Transfer(args) => transfer::run(ctx, &args.tokens).await,
        Command::Link { a, b } => link::link(ctx, a, b).await,
        Command::Unlink { id } => link::unlink(ctx, id).await,
        Command::Balance(args) => balance::run(ctx, &args.tokens).await,
        Command::Account(AccountCommand::Set(args)) => account::set(ctx, &args.tokens).await,
        Command::Import {
            file,
            profile,
            dry_run,
        } => import::run(ctx, &file, &profile, dry_run).await,
        Command::Recur(command) => recur::run(ctx, command).await,
    }
}

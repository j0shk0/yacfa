//! Command-line definitions (clap). Only the shape of the CLI lives here.
//!
//! Most commands take free-form Taskwarrior-style tokens (`+food account:cash`),
//! which are kept as raw strings and interpreted later by `tokens.rs`.
//! Must NOT contain any logic.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// yafca: track income and expenses from the terminal.
#[derive(Parser, Debug)]
#[command(name = "fca", version)]
pub struct Cli {
    /// Database file or sqlite: URL (default: $YAFCA_DB, then config, then the XDG data dir).
    #[arg(long, global = true, env = "YAFCA_DB")]
    pub db: Option<String>,

    /// Config file (default: ~/.config/yafca/config.toml; missing is fine).
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

/// Free-form tokens: an amount and words, `+label`, `-label`, and `key:value` pairs.
#[derive(Args, Debug)]
pub struct TokenArgs {
    #[arg(allow_hyphen_values = true, value_name = "TOKENS")]
    pub tokens: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Add an expense: fca expense 12,50 Groceries +food account:cash payee:Rewe date:2026-11-01
    Expense(TokenArgs),
    /// Add income: fca income 2500 Salary +work account:giro
    Income(TokenArgs),
    /// List transactions: fca list +food account:cash from:D to:D month:2026-10 origin:import type:expense
    List(TokenArgs),
    /// Change a transaction (changing a transfer's amount changes both sides): fca modify 3 +label -label desc:.. amount:-12,50 date:.. account:.. payee:..
    Modify {
        id: i64,
        #[command(flatten)]
        args: TokenArgs,
    },
    /// Delete a transaction by id (a transfer is deleted on both sides).
    Delete { id: i64 },
    /// Show labels with the number of transactions using each.
    Labels,
    /// Move money between two of your accounts: fca transfer 200 from:giro to:savings [Description] [date:D] [+label]
    Transfer(TokenArgs),
    /// Link two existing transactions as both sides of one transfer (e.g. after importing two bank exports).
    Link { a: i64, b: i64 },
    /// Turn a transfer back into two ordinary transactions (give the id of either side).
    Unlink { id: i64 },
    /// Show account balances: fca balance [asof:2026-10-31]
    Balance(TokenArgs),
    /// Account settings.
    #[command(subcommand)]
    Account(AccountCommand),
    /// Import a bank CSV using a profile from the config file.
    Import {
        file: PathBuf,
        /// Profile name from `[import.profiles.<name>]`.
        #[arg(long)]
        profile: String,
        /// Show what would be imported without saving anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Recurring entries, generated automatically as they fall due.
    #[command(subcommand)]
    Recur(RecurCommand),
}

#[derive(Subcommand, Debug)]
pub enum AccountCommand {
    /// Set the starting point of an account, either directly or from what your bank shows today:
    /// fca account set giro opening:1200,00   |   fca account set giro balance:1500,00 [asof:D] [currency:EUR]
    Set(TokenArgs),
}

#[derive(Subcommand, Debug)]
pub enum RecurCommand {
    /// fca recur expense 800 Rent every:1m start:2026-01-01 end:2026-12-31 +home account:giro
    Expense(TokenArgs),
    /// fca recur income 2500 Salary every:1m start:2026-01-25 +work
    Income(TokenArgs),
    /// A recurring transfer between your accounts: fca recur transfer 200 from:giro to:savings every:1m start:2026-01-01 +saving
    Transfer(TokenArgs),
    /// List recurring entries.
    List,
    /// Delete a recurring entry (transactions it already created are kept).
    Delete { id: i64 },
    /// Generate all due transactions now (this also happens before every command).
    Sync,
}

//! `fca recur ...`: add, list, delete and sync recurring entries.
//!
//! Adding reuses `add::parse_entry` for the shared fields and adds the schedule
//! keys `every:`, `start:` and `end:`. `recur transfer` creates a recurring transfer. Must NOT decide which dates are due
//! (that is `fca_core::due_occurrences`, run by the store's sync).

use anyhow::{bail, Context as _, Result};
use chrono::NaiveDate;
use fca_core::{Money, NewRecurring, Schedule};
use fca_store::Store;

use super::add::{parse_entry, Direction};
use super::{Context, Output};
use crate::cli::RecurCommand;
use crate::tokens::{parse_date, Tokens};

pub async fn run(ctx: &Context<'_>, command: RecurCommand) -> Result<Output> {
    match command {
        RecurCommand::Expense(args) => add(ctx, Direction::Expense, &args.tokens).await,
        RecurCommand::Income(args) => add(ctx, Direction::Income, &args.tokens).await,
        RecurCommand::Transfer(args) => add_transfer(ctx, &args.tokens).await,
        RecurCommand::List => Ok(Output::RecurringList(ctx.store.list_recurring().await?)),
        RecurCommand::Delete { id } => {
            ctx.store.delete_recurring(id).await?;
            Ok(Output::RecurringDeleted(id))
        }
        RecurCommand::Sync => Ok(Output::Synced(ctx.store.sync_recurring(ctx.today).await?)),
    }
}

/// `every:`, `start:` and `end:`, shared by all kinds of recurring entries.
struct When {
    schedule: Schedule,
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
}

fn take_when(ctx: &Context<'_>, tokens: &mut Tokens) -> Result<When> {
    let schedule: Schedule = tokens
        .take("every")
        .context("missing schedule, e.g. `every:1m` or `every:2w`")?
        .parse()?;
    let start_date = match tokens.take("start") {
        Some(text) => parse_date(&text, ctx.today)?,
        None => ctx.today,
    };
    let end_date = tokens
        .take("end")
        .map(|text| parse_date(&text, ctx.today))
        .transpose()?;
    if end_date.is_some_and(|end| end < start_date) {
        bail!("end date is before start date");
    }
    Ok(When {
        schedule,
        start_date,
        end_date,
    })
}

async fn add(ctx: &Context<'_>, direction: Direction, args: &[String]) -> Result<Output> {
    let mut tokens = Tokens::parse(args);
    let entry = parse_entry(ctx, &mut tokens, direction)?;
    let when = take_when(ctx, &mut tokens)?;
    tokens.finish()?;

    let new = NewRecurring {
        description: entry.description,
        amount: entry.amount,
        currency: entry.currency,
        account: entry.account,
        payee: entry.payee,
        transfer_to: None,
        schedule: when.schedule,
        start_date: when.start_date,
        end_date: when.end_date,
        labels: entry.labels,
    };
    save(ctx, new).await
}

/// `fca recur transfer <amount> from:<account> to:<account> every:<schedule> [description] ...`
async fn add_transfer(ctx: &Context<'_>, args: &[String]) -> Result<Output> {
    let mut tokens = Tokens::parse(args);
    if !tokens.remove_labels.is_empty() {
        bail!("`-label` only makes sense with `modify`");
    }
    let mut words = tokens.words.iter();
    let amount_text = words
        .next()
        .context("missing amount, e.g. `fca recur transfer 200 from:giro to:savings every:1m`")?;
    let magnitude = Money::parse_auto(amount_text)?.minor().abs();
    let description = words.cloned().collect::<Vec<_>>().join(" ");

    let from = tokens.take("from").context("missing `from:<account>`")?;
    let to = tokens.take("to").context("missing `to:<account>`")?;
    let currency = tokens
        .take("currency")
        .unwrap_or_else(|| ctx.config.currency().to_string());
    let when = take_when(ctx, &mut tokens)?;
    tokens.finish()?;

    let new = NewRecurring {
        // Both sides share one description.
        description: if description.is_empty() {
            format!("Transfer {from} to {to}")
        } else {
            description
        },
        amount: Money::from_minor(-magnitude),
        currency,
        account: from,
        payee: None,
        transfer_to: Some(to),
        schedule: when.schedule,
        start_date: when.start_date,
        end_date: when.end_date,
        labels: tokens.add_labels.clone(),
    };
    save(ctx, new).await
}

async fn save(ctx: &Context<'_>, new: NewRecurring) -> Result<Output> {
    let id = ctx.store.add_recurring(&new).await?;
    // Generate anything already due (e.g. a start date in the past) right away.
    let generated = ctx.store.sync_recurring(ctx.today).await?;
    Ok(Output::RecurringAdded {
        id,
        description: new.description,
        schedule: new.schedule.to_string(),
        generated,
    })
}

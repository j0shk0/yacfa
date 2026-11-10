//! `fca account set <name> opening:<amount> | balance:<amount> [asof:D] [currency:C]`.
//!
//! `opening:` sets the balance before the first recorded transaction. `balance:`
//! is the easier way to start: say what your bank shows on a date and the opening
//! balance is worked out from the transactions recorded up to then.

use anyhow::{bail, Result};
use fca_core::{same_account, Account, Money};
use fca_store::Store;

use super::{Context, Output};
use crate::tokens::{parse_date, Tokens};

pub async fn set(ctx: &Context<'_>, args: &[String]) -> Result<Output> {
    let mut tokens = Tokens::parse(args);
    if !tokens.add_labels.is_empty() || !tokens.remove_labels.is_empty() {
        bail!("accounts do not have labels");
    }
    let name = match tokens.words.as_slice() {
        [name] => name.clone(),
        [] => bail!("missing account name, e.g. `fca account set giro balance:1500,00`"),
        _ => bail!("expected one account name; quote names with spaces"),
    };
    let opening = tokens
        .take("opening")
        .map(|t| Money::parse_auto(&t))
        .transpose()?;
    let balance = tokens
        .take("balance")
        .map(|t| Money::parse_auto(&t))
        .transpose()?;
    let asof = tokens
        .take("asof")
        .map(|t| parse_date(&t, ctx.today))
        .transpose()?;
    let currency = tokens.take("currency");
    tokens.finish()?;

    if opening.is_some() && balance.is_some() {
        bail!("use either `opening:` or `balance:`, not both");
    }
    if asof.is_some() && balance.is_none() {
        bail!("`asof:` only works together with `balance:`");
    }
    if opening.is_none() && balance.is_none() && currency.is_none() {
        bail!("nothing to set: give `opening:`, `balance:` or `currency:`");
    }

    let existing = ctx
        .store
        .list_accounts()
        .await?
        .into_iter()
        .find(|a| same_account(&a.name, &name));
    let currency = currency
        .or_else(|| existing.as_ref().map(|a| a.currency.clone()))
        .unwrap_or_else(|| ctx.config.currency().to_string());
    let previous_opening = existing
        .as_ref()
        .map_or(Money::default(), |a| a.opening_balance);

    let opening_balance = match (opening, balance) {
        (Some(opening), _) => opening,
        (None, Some(target)) => {
            // Shift the opening balance by however far the computed balance is off.
            let asof = asof.unwrap_or(ctx.today);
            let current = ctx
                .store
                .balances(asof)
                .await?
                .into_iter()
                .find(|b| same_account(&b.account, &name) && b.currency == currency)
                .map_or(Money::default(), |b| b.amount);
            previous_opening + (target + -current)
        }
        (None, None) => previous_opening,
    };

    let account = Account {
        name: existing.map_or(name, |a| a.name),
        currency,
        opening_balance,
    };
    ctx.store.set_account(&account).await?;
    Ok(Output::AccountSet(account))
}

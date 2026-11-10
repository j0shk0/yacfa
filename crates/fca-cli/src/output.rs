//! Rendering: the one place that turns command results into text.
//!
//! `render` returns a `String`; `main` prints it. Must NOT query the store
//! or make decisions about data.

use std::collections::BTreeMap;
use std::fmt::Write;

use comfy_table::{presets::UTF8_FULL_CONDENSED, Cell, CellAlignment, Table};
use fca_core::{Balance, Money, NewTransaction, Recurring, Transaction};
use fca_store::LabelCount;

use crate::commands::Output;

pub fn render(output: &Output) -> String {
    match output {
        Output::Created {
            id,
            amount,
            currency,
            description,
        } => {
            format!("Created transaction {id}: {amount} {currency} {description}\n")
        }
        Output::Modified(t) => format!(
            "Modified transaction {}: {} {} {}\n",
            t.id, t.amount, t.currency, t.description
        ),
        Output::Deleted(ids) => match ids.as_slice() {
            [id] => format!("Deleted transaction {id}\n"),
            _ => {
                let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
                format!(
                    "Deleted transactions {} (both sides of a transfer)\n",
                    ids.join(" and ")
                )
            }
        },
        Output::TransferCreated {
            out_id,
            in_id,
            amount,
            currency,
            from,
            to,
        } => {
            format!("Transferred {amount} {currency} from {from} to {to} (transactions {out_id} and {in_id})\n")
        }
        Output::Linked { a, b } => format!("Linked transactions {a} and {b} as a transfer\n"),
        Output::Unlinked(id) => {
            format!("Transaction {id} and its counterpart are no longer a transfer\n")
        }
        Output::Balances { asof, balances } => balances_table(*asof, balances),
        Output::AccountSet(a) => {
            format!(
                "Account {}: opening balance {} {}\n",
                a.name, a.opening_balance, a.currency
            )
        }
        Output::Transactions(list) => transactions_table(list),
        Output::Labels(labels) => labels_table(labels),
        Output::ImportPreview(list) => preview_table(list),
        Output::Imported { inserted, skipped } => {
            format!("Imported {inserted} transactions, skipped {skipped} already imported\n")
        }
        Output::RecurringAdded {
            id,
            description,
            schedule,
            generated,
        } => {
            format!("Created recurring entry {id}: {description} every {schedule} ({generated} transactions generated)\n")
        }
        Output::RecurringList(list) => recurring_table(list),
        Output::RecurringDeleted(id) => format!("Deleted recurring entry {id}\n"),
        Output::Synced(count) => format!("Generated {count} transactions\n"),
    }
}

fn transactions_table(list: &[Transaction]) -> String {
    if list.is_empty() {
        return "No transactions.\n".to_string();
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header([
        "ID",
        "Date",
        "Amount",
        "Cur",
        "Account",
        "Payee",
        "Description",
        "Labels",
        "Origin",
        "Xfer",
    ]);
    let mut totals: BTreeMap<&str, Money> = BTreeMap::new();
    for t in list {
        table.add_row([
            t.id.to_string(),
            t.date.to_string(),
            t.amount.to_string(),
            t.currency.clone(),
            t.account.clone(),
            t.payee.clone().unwrap_or_default(),
            t.description.clone(),
            t.labels.join(" "),
            t.origin.to_string(),
            t.transfer_id.map(|id| id.to_string()).unwrap_or_default(),
        ]);
        *totals.entry(t.currency.as_str()).or_default() =
            totals.get(t.currency.as_str()).copied().unwrap_or_default() + t.amount;
    }
    // Totals are per currency: adding EUR to USD would be meaningless.
    for (currency, total) in totals {
        table.add_row([
            Cell::new(""),
            Cell::new("Total"),
            Cell::new(total.to_string()),
            Cell::new(currency),
        ]);
    }
    if let Some(column) = table.column_mut(2) {
        column.set_cell_alignment(CellAlignment::Right);
    }
    let mut out = table.to_string();
    let _ = writeln!(out);
    out
}

fn labels_table(labels: &[LabelCount]) -> String {
    if labels.is_empty() {
        return "No labels.\n".to_string();
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Label", "Transactions"]);
    for l in labels {
        table.add_row([l.name.clone(), l.count.to_string()]);
    }
    format!("{table}\n")
}

fn recurring_table(list: &[Recurring]) -> String {
    if list.is_empty() {
        return "No recurring entries.\n".to_string();
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header([
        "ID",
        "Description",
        "Amount",
        "Cur",
        "Account",
        "Payee",
        "Every",
        "Start",
        "End",
        "Last",
        "Labels",
    ]);
    for r in list {
        let date = |d: Option<chrono::NaiveDate>| d.map(|d| d.to_string()).unwrap_or_default();
        let description = if r.active {
            r.description.clone()
        } else {
            format!("{} (inactive)", r.description)
        };
        table.add_row([
            r.id.to_string(),
            description,
            r.amount.to_string(),
            r.currency.clone(),
            match &r.transfer_to {
                Some(to) => format!("{} -> {to}", r.account),
                None => r.account.clone(),
            },
            r.payee.clone().unwrap_or_default(),
            r.schedule.to_string(),
            r.start_date.to_string(),
            date(r.end_date),
            date(r.last_generated),
            r.labels.join(" "),
        ]);
    }
    format!("{table}\n")
}

/// `--dry-run` view: like the list table but without ids, with the file's totals.
fn preview_table(list: &[NewTransaction]) -> String {
    if list.is_empty() {
        return "Nothing to import.\n".to_string();
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header([
        "Date",
        "Amount",
        "Cur",
        "Account",
        "Payee",
        "Description",
        "Labels",
    ]);
    let mut total = Money::default();
    for t in list {
        table.add_row([
            t.date.to_string(),
            t.amount.to_string(),
            t.currency.clone(),
            t.account.clone(),
            t.payee.clone().unwrap_or_default(),
            t.description.clone(),
            t.labels.join(" "),
        ]);
        total = total + t.amount;
    }
    format!(
        "{table}\n{} rows, sum {total} (dry run, nothing saved)\n",
        list.len()
    )
}

fn balances_table(asof: chrono::NaiveDate, balances: &[Balance]) -> String {
    if balances.is_empty() {
        return "No accounts yet.\n".to_string();
    }
    let mut table = Table::new();
    table.load_preset(UTF8_FULL_CONDENSED);
    table.set_header(["Account", "Cur", "Balance"]);
    let mut totals: BTreeMap<&str, Money> = BTreeMap::new();
    for b in balances {
        table.add_row([b.account.clone(), b.currency.clone(), b.amount.to_string()]);
        let total = totals.entry(b.currency.as_str()).or_default();
        *total = *total + b.amount;
    }
    for (currency, total) in totals {
        table.add_row(["Total".to_string(), currency.to_string(), total.to_string()]);
    }
    if let Some(column) = table.column_mut(2) {
        column.set_cell_alignment(CellAlignment::Right);
    }
    format!("Balances as of {asof}\n{table}\n")
}

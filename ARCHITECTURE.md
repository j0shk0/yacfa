# yafca architecture

`yafca` ("yet another finance cli app") tracks income and expenses from the
terminal. The binary is called `fca`.

## Crate graph

```
fca-cli ──► fca-store ──► fca-core
   └──────► fca-import ──► fca-core
```

Dependencies point one way only, so each crate can be reviewed on its own.

| Crate        | One job                                                   | May do IO? |
|--------------|-----------------------------------------------------------|------------|
| `fca-core`   | Domain types and rules (money, schedules, filters)        | No         |
| `fca-store`  | Persist and query via the `Store` trait (SQLite for now)  | DB only    |
| `fca-import` | Bank CSV bytes + profile → `Vec<NewTransaction>`          | No         |
| `fca-cli`    | Parse arguments, call the others, print results           | Yes        |

`fca-import` never touches the database. The CLI passes its output to the store.

## Data flow of a command

1. `main.rs` parses arguments (`cli.rs`), loads the config (`config.rs`), opens the store.
2. It runs the recurring sync (generates due recurring transactions up to today).
3. It dispatches to a function in `commands/`, which turns free-form tokens
   (`tokens.rs`) into core types, calls the store, and **returns data**.
4. `output.rs` renders that data. Nothing else prints.

`fca import` follows the same shape: the CLI reads the file, `fca-import`
turns bytes + a profile (from the config file) into `NewTransaction`s with a stable
`import_hash`, and the store inserts them, skipping hashes it has seen.

Recurring entries: `fca-core::due_occurrences` decides which dates are due (pure),
`Store::sync_recurring` only persists them, and `main.rs` runs it before every command.

Accounts and transfers: an account is just a name on transactions, plus an optional
`accounts` row holding its opening balance. A balance is computed, never stored:
SQL sums transactions, `fca_core::compute_balances` adds the opening balance. A transfer
is two ordinary transactions sharing a `transfer_id`; `fca_core::transfer` defines what a
valid transfer is, the store saves it and keeps both sides in step on modify and delete.
A recurring entry with `transfer_to` set generates a transfer per occurrence; only the outgoing
side carries the `recurring_id`, which keeps the `(recurring_id, date)` uniqueness (and so
idempotent sync) intact.

## Where to add things

- **A new command:** add a variant in `cli.rs`, a file in `commands/`, and a match arm in `commands/mod.rs`.
- **A bank profile:** add a `[import.profiles.<name>]` table to the config file; no code needed.
- **Postgres:** implement `Store` in a new `fca-store/src/postgres/` module with its own migrations.
  `commands::Context` currently holds a concrete `&SqliteStore` (async trait methods are not
  object-safe); at that point make `Context` generic over `S: Store` or use an enum of backends.
- **A schedule type (e.g. RRULE):** add a variant to `fca-core::Schedule`; nothing else changes.
- **A TUI:** add an `fca-tui` crate depending on core/store/import, like `fca-cli` does.

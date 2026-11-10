# yafca

> This project was completely AI generated... A lot of refactoring can be expected in the future during clean up.
> At the current stage I can't say much about its quality so don't rely too much on it and be careful. For more see the [AI usage](#ai-usage) section.

*Yet another finance CLI app.* A small, Taskwarrior-style command line tool
for tracking income and expenses. The binary is called `fca`.

- Entries have labels, an account, and an optional payee.
- Recurring entries (rent, salary, subscriptions) follow arbitrary schedules and are generated automatically.
- Bank CSV exports can be imported through profiles defined in TOML.
- Data lives in a local SQLite file.

## Build and run

```sh
cargo build --release          # binary: target/release/fca
cargo run -p fca-cli -- --help
cargo test --workspace
```

## Usage

Arguments after the subcommand are free-form tokens: an amount, words for the
description, `+label`, `-label` (only for `modify`), and `key:value` pairs.

```sh
fca expense 12,50 Groceries +food account:cash payee:Rewe date:2026-11-01
fca income 2500 Salary +work account:giro

fca list                                  # everything, with a total per currency
fca list +food account:cash month:2026-10
fca list from:2026-01-01 to:today type:expense origin:import

fca modify 3 +travel -food desc:"Train ticket" amount:-45,90 date:yesterday payee:
fca delete 3
fca labels                                # labels with counts; created implicitly
```

Amounts accept `12,50`, `12.50`, `1.234,56` and `1,234.56`. `expense` and
`income` set the sign for you; in `modify`, `amount:` is taken literally
(negative is an expense). `payee:` with an empty value clears the payee.
Dates are `YYYY-MM-DD`, `today`, `yesterday` or `tomorrow`.

### Accounts, balances and transfers

```sh
fca account set giro balance:1500,00     # "my bank shows 1500.00 today"
fca account set giro opening:1200,00     # or: the balance before the first recorded transaction
fca balance                              # every account, plus a total per currency
fca balance asof:2026-10-31

fca transfer 200 from:giro to:savings +saving date:2026-11-01
fca list type:transfer
```

A balance is the account's opening balance plus all its transactions up to the
date. `balance:` works the opening balance out for you from what is already
recorded, so you can set it at any time and it stays correct.

A transfer is two linked transactions: money out of one account and into the
other. Transfers move balances but never count as income or expense (`type:income`
and `type:expense` skip them), so your total stays the same. Changing the amount
of one side changes the other, and deleting one side deletes both.

When both sides come from bank imports (say a credit card payment shows up in two
exports), pair them up afterwards. The amounts must be equal with opposite signs,
in different accounts; the dates may differ:

```sh
fca link 12 40       # make these two transactions one transfer
fca unlink 12        # undo it
```

### Recurring entries

```sh
fca recur expense 800 Rent every:1m start:2026-01-01 end:2026-12-31 +home account:giro
fca recur income 2500 Salary every:1m start:2026-01-25 +work
fca recur transfer 150 from:giro to:savings every:1m +saving   # a standing order
fca recur list
fca recur delete 1       # transactions it already created are kept
fca recur sync           # generate everything due now
```

Schedules: `daily`, `weekly`, `monthly`, `quarterly`, `yearly`, `every 2 weeks`,
or the short forms `10d`, `2w`, `3m`, `1y`. Due transactions are generated
lazily before every command, and running the sync twice never creates
duplicates. A monthly entry starting on the 31st stays at month-end.

### Importing bank CSVs

```sh
fca --config config.example.toml import statement.csv --profile mybank --dry-run
fca --config config.example.toml import statement.csv --profile mybank
```

Importing the same file again adds nothing; identical rows within one file
(two coffees on the same day) are both kept. Profiles describe delimiter,
encoding, date format, decimal/thousands separators and column mapping. See
[`config.example.toml`](config.example.toml) for a German/EU bank example
(`;`, `1.234,56`, `dd.mm.yyyy`, windows-1252).

## Configuration

The config file is optional: `~/.config/yafca/config.toml`, or `--config <path>`
(an explicitly given file must exist). Keys: `database_url`, `default_account`,
`default_currency`, and `[import.profiles.<name>]` tables.

The database location is, in order of precedence: `--db <path>`, `$YAFCA_DB`,
`database_url` in the config, then `$XDG_DATA_HOME/yafca/yafca.db`.

## Project layout

A Cargo workspace of four small crates; dependencies point one way only.

| Crate        | Job                                                |
|--------------|----------------------------------------------------|
| `fca-core`   | Pure domain types and rules (money, schedules)     |
| `fca-store`  | `Store` trait and its SQLite implementation        |
| `fca-import` | Bank CSV + profile into transactions               |
| `fca-cli`    | The `fca` binary: arguments, commands, output      |

See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the crate graph, the data flow of
a command, and where to add a command, a bank profile, Postgres, or a TUI.

## AI usage

The code, tests and documentation of this project were generated by AI
(Anthropic's Claude, used through Claude Code), not written by hand.

**What the human did:** came up with the idea, set the requirements (a
Taskwarrior-style CLI for expenses with labels, recurring entries, bank CSV
import and SQLite storage), chose the name `fca`, and made the design decisions
along the way. The plan was written first and adjusted after feedback before any
code was generated. One explicit requirement was that the project stays
reviewable by a person: small crates, small files, and a doc comment at the top
of every file.

**What the AI did:** wrote the plan, the code, the tests and this
documentation, based on those requirements.

**What this means for you:**

- The code has **not been fully reviewed** by a human. Expect bugs, especially
  in the parts where mistakes are easy to miss: money parsing and rounding,
  recurring dates, import deduplication and balance calculation.
- The tests were also written by the AI, so they can share its blind spots.
  Passing tests are not proof of correctness.
- Don't base real financial decisions on the output yet. Back up your database
  file, and check imports with `--dry-run` before writing anything.
- A lot of refactoring is expected as the code gets reviewed and cleaned up.

The split into crates and [`ARCHITECTURE.md`](ARCHITECTURE.md) exist to make
that review possible in small steps.

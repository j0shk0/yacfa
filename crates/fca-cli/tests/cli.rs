//! End-to-end tests: run the real `fca` binary against a temporary database.

use assert_cmd::Command;
use tempfile::TempDir;

/// Run `fca --db <tmp>/t.db <args>` and return (success, stdout, stderr).
fn fca(dir: &TempDir, args: &[&str]) -> (bool, String, String) {
    let db = dir.path().join("t.db");
    let out = Command::cargo_bin("fca")
        .unwrap()
        .env_remove("YAFCA_DB")
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let (success, stdout, stderr) = fca(dir, args);
    assert!(success, "fca {args:?} failed: {stderr}");
    stdout
}

#[test]
fn add_list_modify_delete() {
    let dir = TempDir::new().unwrap();
    ok(
        &dir,
        &[
            "expense",
            "12,50",
            "Groceries",
            "+food",
            "account:cash",
            "payee:Rewe",
            "date:2026-10-01",
        ],
    );
    ok(
        &dir,
        &[
            "income",
            "2500",
            "Salary",
            "+work",
            "account:giro",
            "date:2026-10-02",
        ],
    );

    let food = ok(&dir, &["list", "+food"]);
    assert!(
        food.contains("Groceries") && food.contains("-12.50") && !food.contains("Salary"),
        "{food}"
    );

    let all = ok(&dir, &["list", "month:2026-10"]);
    assert!(all.contains("Total") && all.contains("2487.50"), "{all}");
    assert!(ok(&dir, &["list", "type:income"]).contains("Salary"));

    let modified = ok(
        &dir,
        &[
            "modify",
            "1",
            "-food",
            "+groceries",
            "desc:Weekly shop",
            "amount:-13,00",
        ],
    );
    assert!(
        modified.contains("-13.00") && modified.contains("Weekly shop"),
        "{modified}"
    );
    let labels = ok(&dir, &["labels"]);
    assert!(
        labels.contains("groceries") && !labels.contains("food"),
        "{labels}"
    );

    ok(&dir, &["delete", "1"]);
    assert!(!ok(&dir, &["list"]).contains("Weekly shop"));
}

#[test]
fn helpful_errors() {
    let dir = TempDir::new().unwrap();
    let (success, _, stderr) = fca(&dir, &["expense", "Groceries"]);
    assert!(!success && stderr.contains("invalid amount"), "{stderr}");
    let (success, _, stderr) = fca(&dir, &["list", "acount:cash"]);
    assert!(!success && stderr.contains("unknown attribute"), "{stderr}");
    let (success, _, stderr) = fca(&dir, &["delete", "42"]);
    assert!(!success && stderr.contains("not found"), "{stderr}");
}

#[test]
fn recurring_entries_are_generated_lazily_and_only_once() {
    use chrono::{Days, Local};
    let dir = TempDir::new().unwrap();
    let start = Local::now()
        .date_naive()
        .checked_sub_days(Days::new(21))
        .unwrap();
    let start_arg = format!("start:{start}");

    let added = ok(
        &dir,
        &[
            "recur", "expense", "20", "Gym", "every:1w", &start_arg, "+sport",
        ],
    );
    assert!(added.contains("4 transactions generated"), "{added}");

    let rows = ok(&dir, &["list", "origin:recurring"]);
    assert_eq!(rows.matches("Gym").count(), 4, "{rows}");
    assert!(rows.contains("-80.00"), "{rows}");

    // Any later command syncs again, but nothing new is due.
    assert!(ok(&dir, &["recur", "sync"]).contains("Generated 0"));
    assert_eq!(
        ok(&dir, &["list", "origin:recurring"])
            .matches("Gym")
            .count(),
        4
    );

    assert!(ok(&dir, &["recur", "list"]).contains("1w"));
    ok(&dir, &["recur", "delete", "1"]);
    assert!(ok(&dir, &["recur", "list"]).contains("No recurring"));
    assert_eq!(
        ok(&dir, &["list", "origin:recurring"])
            .matches("Gym")
            .count(),
        4,
        "history is kept"
    );
}

#[test]
fn recur_requires_a_schedule() {
    let dir = TempDir::new().unwrap();
    let (success, _, stderr) = fca(&dir, &["recur", "expense", "20", "Gym"]);
    assert!(!success && stderr.contains("missing schedule"), "{stderr}");
}

/// Path of a file shipped in the repository, relative to the workspace root.
fn repo_file(relative: &str) -> String {
    format!("{}/../../{relative}", env!("CARGO_MANIFEST_DIR"))
}

fn import_args<'a>(csv: &'a str, config: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["--config", config, "import", csv, "--profile", "mybank"];
    args.extend_from_slice(extra);
    args
}

#[test]
fn import_is_idempotent_and_dry_run_saves_nothing() {
    let dir = TempDir::new().unwrap();
    let csv = repo_file("crates/fca-import/tests/fixtures/eu_bank.csv");
    let config = repo_file("config.example.toml");

    let preview = ok(&dir, &import_args(&csv, &config, &["--dry-run"]));
    assert!(
        preview.contains("Bäckerei Schröder") && preview.contains("dry run"),
        "{preview}"
    );
    assert!(ok(&dir, &["list"]).contains("No transactions"));

    let first = ok(&dir, &import_args(&csv, &config, &[]));
    assert!(
        first.contains("Imported 5") && first.contains("skipped 0"),
        "{first}"
    );
    let second = ok(&dir, &import_args(&csv, &config, &[]));
    assert!(
        second.contains("Imported 0") && second.contains("skipped 5"),
        "{second}"
    );

    let october = ok(
        &dir,
        &["list", "month:2026-10", "origin:import", "+imported"],
    );
    assert_eq!(october.matches("Bäckerei Schröder").count(), 2, "{october}");
    assert!(october.contains("2777.56"), "{october}"); // -950 + 2500 - 3.50 - 3.50 + 1234.56
}

#[test]
fn import_reports_unknown_profile() {
    let dir = TempDir::new().unwrap();
    let csv = repo_file("crates/fca-import/tests/fixtures/eu_bank.csv");
    let config = repo_file("config.example.toml");
    let (success, _, stderr) = fca(
        &dir,
        &["--config", &config, "import", &csv, "--profile", "nope"],
    );
    assert!(!success && stderr.contains("available: mybank"), "{stderr}");
}

#[test]
fn transfers_move_balances_but_are_not_income_or_expense() {
    let dir = TempDir::new().unwrap();
    ok(&dir, &["account", "set", "giro", "opening:1000,00"]);
    ok(
        &dir,
        &[
            "expense",
            "50",
            "Groceries",
            "account:giro",
            "date:2026-10-01",
        ],
    );
    let made = ok(
        &dir,
        &[
            "transfer",
            "200",
            "from:giro",
            "to:savings",
            "date:2026-10-02",
            "+saving",
        ],
    );
    assert!(
        made.contains("Transferred 200.00 EUR from giro to savings"),
        "{made}"
    );

    let balances = ok(&dir, &["balance"]);
    assert!(
        balances.contains("750.00") && balances.contains("200.00"),
        "{balances}"
    );
    assert!(
        balances.contains("Total") && balances.contains("950.00"),
        "{balances}"
    );
    // As of the day before the transfer, savings does not exist yet.
    let earlier = ok(&dir, &["balance", "asof:2026-10-01"]);
    assert!(
        earlier.contains("950.00") && !earlier.contains("savings"),
        "{earlier}"
    );

    assert!(!ok(&dir, &["list", "type:expense"]).contains("Transfer to savings"));
    assert!(!ok(&dir, &["list", "type:income"]).contains("Transfer from giro"));
    let transfers = ok(&dir, &["list", "type:transfer"]);
    assert!(
        transfers.contains("Transfer to savings") && transfers.contains("Transfer from giro"),
        "{transfers}"
    );

    // Changing one side changes the other; deleting one side deletes both.
    ok(&dir, &["modify", "2", "amount:-250,00"]);
    assert!(ok(&dir, &["balance"]).contains("250.00"));
    let deleted = ok(&dir, &["delete", "3"]);
    assert!(deleted.contains("both sides"), "{deleted}");
    assert!(ok(&dir, &["list", "type:transfer"]).contains("No transactions"));
}

#[test]
fn account_balance_can_be_set_from_the_bank_statement() {
    let dir = TempDir::new().unwrap();
    ok(
        &dir,
        &["expense", "30", "Shoes", "account:giro", "date:2026-10-01"],
    );
    ok(
        &dir,
        &["income", "100", "Gift", "account:giro", "date:2026-10-02"],
    );
    // The bank says 1500.00 today, so the opening balance must have been 1430.00.
    let set = ok(&dir, &["account", "set", "giro", "balance:1500,00"]);
    assert!(set.contains("opening balance 1430.00"), "{set}");
    assert!(ok(&dir, &["balance"]).contains("1500.00"));
    let (success, _, stderr) = fca(&dir, &["account", "set", "giro", "opening:1", "balance:2"]);
    assert!(!success && stderr.contains("not both"), "{stderr}");
}

#[test]
fn link_pairs_existing_transactions_as_a_transfer() {
    let dir = TempDir::new().unwrap();
    ok(
        &dir,
        &[
            "expense",
            "50",
            "Card payment",
            "account:giro",
            "date:2026-10-01",
        ],
    );
    ok(
        &dir,
        &[
            "income",
            "50",
            "Payment received",
            "account:credit",
            "date:2026-10-03",
        ],
    );
    ok(
        &dir,
        &["income", "40", "Other", "account:credit", "date:2026-10-03"],
    );

    let (success, _, stderr) = fca(&dir, &["link", "1", "3"]);
    assert!(!success && stderr.contains("opposite signs"), "{stderr}");
    ok(&dir, &["link", "1", "2"]);
    let transfers = ok(&dir, &["list", "type:transfer"]);
    assert!(
        transfers.contains("Card payment") && transfers.contains("Payment received"),
        "{transfers}"
    );
    assert!(!transfers.contains("Other"));

    ok(&dir, &["unlink", "2"]);
    assert!(ok(&dir, &["list", "type:transfer"]).contains("No transactions"));
}

#[test]
fn transfer_errors_are_clear() {
    let dir = TempDir::new().unwrap();
    let (success, _, stderr) = fca(&dir, &["transfer", "10", "from:giro"]);
    assert!(!success && stderr.contains("missing `to:"), "{stderr}");
    let (success, _, stderr) = fca(&dir, &["transfer", "10", "from:giro", "to:GIRO"]);
    assert!(
        !success && stderr.contains("two different accounts"),
        "{stderr}"
    );
}

#[test]
fn recurring_transfers_move_money_on_schedule() {
    use chrono::{Days, Local};
    let dir = TempDir::new().unwrap();
    let start = Local::now()
        .date_naive()
        .checked_sub_days(Days::new(14))
        .unwrap();
    let start_arg = format!("start:{start}");

    let added = ok(
        &dir,
        &[
            "recur",
            "transfer",
            "100",
            "from:giro",
            "to:savings",
            "every:1w",
            &start_arg,
            "+saving",
        ],
    );
    assert!(added.contains("6 transactions generated"), "{added}"); // 3 weeks, 2 sides each

    let balances = ok(&dir, &["balance"]);
    assert!(
        balances.contains("-300.00") && balances.contains("300.00") && balances.contains("0.00"),
        "{balances}"
    );
    assert!(ok(&dir, &["list", "type:expense"]).contains("No transactions"));
    assert_eq!(
        ok(&dir, &["list", "type:transfer", "origin:recurring"])
            .matches("Transfer giro to savings")
            .count(),
        6
    );

    // Nothing new is due, and the list shows where the money goes.
    assert!(ok(&dir, &["recur", "sync"]).contains("Generated 0"));
    assert!(ok(&dir, &["recur", "list"]).contains("giro -> savings"));

    let (success, _, stderr) = fca(
        &dir,
        &["recur", "transfer", "5", "from:a", "to:A", "every:1m"],
    );
    assert!(
        !success && stderr.contains("two different accounts"),
        "{stderr}"
    );
}

//! Integration test: the shipped example profile reads the shipped EU bank fixture.

use fca_import::{import_csv, ImportConfig};
use serde::Deserialize;

#[derive(Deserialize)]
struct ConfigFile {
    import: ImportConfig,
}

fn mybank_profile() -> fca_import::ImportProfile {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config.example.toml"
    ))
    .unwrap();
    let mut file: ConfigFile = toml::from_str(&text).unwrap();
    file.import
        .profiles
        .remove("mybank")
        .expect("config.example.toml defines `mybank`")
}

const FIXTURE: &[u8] = include_bytes!("fixtures/eu_bank.csv");

#[test]
fn reads_windows_1252_fixture_with_umlauts() {
    let rows = import_csv(FIXTURE, &mybank_profile()).unwrap();
    assert_eq!(rows.len(), 5);

    let rent = &rows[0];
    assert_eq!(rent.date.to_string(), "2026-10-01");
    assert_eq!(rent.amount.minor(), -95000);
    assert_eq!(rent.description, "Miete Oktober / Dauerauftrag");
    assert_eq!(rent.payee.as_deref(), Some("Müller Hausverwaltung"));
    assert_eq!(rent.account, "Girokonto");
    assert_eq!(rent.labels, ["imported"]);

    assert_eq!(rows[1].amount.minor(), 250000);
    assert_eq!(rows[2].description, "Brötchen und Gebäck / Kartenzahlung");
    assert_eq!(rows[4].amount.minor(), 123456);
    assert_eq!(rows[4].payee.as_deref(), Some("Stadtwerke Köln"));
}

#[test]
fn same_day_duplicates_are_kept_with_distinct_hashes() {
    let rows = import_csv(FIXTURE, &mybank_profile()).unwrap();
    let (a, b) = (
        rows[2].import_hash.as_ref().unwrap(),
        rows[3].import_hash.as_ref().unwrap(),
    );
    assert_ne!(a, b);
    // Same file again gives the same hashes, which is what makes re-import a no-op.
    let again = import_csv(FIXTURE, &mybank_profile()).unwrap();
    assert_eq!(again[2].import_hash.as_ref(), Some(a));
}

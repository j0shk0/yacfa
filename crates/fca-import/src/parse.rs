//! Step 2 of import: CSV text to `NewTransaction`s using the profile's column mapping.
//!
//! Errors carry the line number of the offending row in the original file.
//! Must NOT decode bytes (see `decode.rs`) or compute hashes (see `dedup.rs`).

use chrono::NaiveDate;
use csv::{ReaderBuilder, StringRecord, Trim};
use fca_core::{Money, NewTransaction, Origin};

use crate::profile::ImportProfile;
use crate::ImportError;

/// Column positions resolved from the header row.
struct Resolved {
    date: usize,
    amount: Option<usize>,
    debit: Option<usize>,
    credit: Option<usize>,
    payee: Option<usize>,
    description: Vec<usize>,
}

impl Resolved {
    fn new(header: &StringRecord, profile: &ImportProfile) -> Result<Resolved, ImportError> {
        let find = |name: &str| {
            header
                .iter()
                .position(|h| h == name)
                .ok_or_else(|| ImportError::MissingColumn {
                    name: name.to_string(),
                    found: header.iter().collect::<Vec<_>>().join(", "),
                })
        };
        let optional = |name: &Option<String>| name.as_deref().map(find).transpose();
        let c = &profile.columns;
        Ok(Resolved {
            date: find(&c.date)?,
            amount: optional(&c.amount)?,
            debit: optional(&c.debit)?,
            credit: optional(&c.credit)?,
            payee: optional(&c.payee)?,
            description: c
                .description
                .iter()
                .map(|n| find(n))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Parse the (already decoded and preamble-free) CSV text.
pub fn parse_records(
    text: &str,
    profile: &ImportProfile,
) -> Result<Vec<NewTransaction>, ImportError> {
    let mut reader = ReaderBuilder::new()
        .delimiter(profile.delimiter as u8) // validated to be ASCII
        .has_headers(true)
        .flexible(true)
        .trim(Trim::All)
        .from_reader(text.as_bytes());
    let columns = Resolved::new(reader.headers()?, profile)?;

    let mut out = Vec::new();
    for record in reader.records() {
        let record = record?;
        // Lines are counted from the start of the text we were given; add the dropped preamble.
        let line = record.position().map_or(0, |p| p.line()) + profile.skip_rows as u64;
        let tx = parse_row(&record, &columns, profile)
            .map_err(|message| ImportError::Row { line, message })?;
        out.push(tx);
    }
    Ok(out)
}

fn field(record: &StringRecord, index: usize) -> Result<&str, String> {
    record
        .get(index)
        .ok_or_else(|| "row has too few columns".to_string())
}

fn parse_row(
    record: &StringRecord,
    cols: &Resolved,
    profile: &ImportProfile,
) -> Result<NewTransaction, String> {
    let date_text = field(record, cols.date)?;
    let date = NaiveDate::parse_from_str(date_text, &profile.date_format).map_err(|_| {
        format!(
            "invalid date {date_text:?} (expected format {})",
            profile.date_format
        )
    })?;

    let payee = match cols.payee {
        Some(i) => Some(field(record, i)?.to_string()).filter(|p| !p.is_empty()),
        None => None,
    };
    let parts: Vec<&str> = cols
        .description
        .iter()
        .map(|&i| field(record, i))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();

    Ok(NewTransaction {
        date,
        amount: parse_amount(record, cols, profile)?,
        currency: profile.currency.clone(),
        description: parts.join(" / "),
        account: profile.account.clone(),
        payee,
        origin: Origin::Import,
        recurring_id: None,
        import_hash: None,
        labels: profile.labels.clone(),
    })
}

fn parse_amount(
    record: &StringRecord,
    cols: &Resolved,
    profile: &ImportProfile,
) -> Result<Money, String> {
    let parse = |text: &str| {
        Money::parse(text, profile.decimal_separator, profile.thousands())
            .map_err(|e| e.to_string())
    };
    if let Some(i) = cols.amount {
        return parse(field(record, i)?);
    }
    // Separate money-out / money-in columns: both are positive numbers, empty means none.
    let read = |index: Option<usize>| -> Result<Money, String> {
        match index.map(|i| field(record, i)).transpose()? {
            Some(text) if !text.is_empty() => Ok(Money::from_minor(parse(text)?.minor().abs())),
            _ => Ok(Money::default()),
        }
    };
    let (debit, credit) = (read(cols.debit)?, read(cols.credit)?);
    if debit == Money::default() && credit == Money::default() {
        return Err("no amount in debit/credit columns".to_string());
    }
    Ok(credit + -debit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(toml_text: &str) -> ImportProfile {
        toml::from_str(toml_text).unwrap()
    }

    const SINGLE: &str = r#"
        delimiter = ";"
        date_format = "%d.%m.%Y"
        decimal_separator = ","
        thousands_separator = "."
        account = "giro"
        labels = ["imported"]
        [columns]
        date = "Datum"
        amount = "Betrag"
        payee = "Name"
        description = ["Zweck", "Text"]
    "#;

    #[test]
    fn maps_columns_and_joins_description() {
        let csv =
            "Datum;Betrag;Name;Zweck;Text\n01.10.2026;-1.234,56;Vermieter;Miete;Lastschrift\n";
        let rows = parse_records(csv, &profile(SINGLE)).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.amount.minor(), -123456);
        assert_eq!(r.date.to_string(), "2026-10-01");
        assert_eq!(
            (r.description.as_str(), r.payee.as_deref()),
            ("Miete / Lastschrift", Some("Vermieter"))
        );
        assert_eq!(
            (r.origin, r.labels.clone(), r.account.as_str()),
            (Origin::Import, vec!["imported".to_string()], "giro")
        );
    }

    #[test]
    fn debit_and_credit_columns() {
        let p = profile(
            r#"
            date_format = "%Y-%m-%d"
            account = "a"
            [columns]
            date = "d"
            debit = "out"
            credit = "in"
            description = ["t"]
        "#,
        );
        let rows = parse_records(
            "d,out,in,t\n2026-10-01,5.00,,Shop\n2026-10-02,,7.50,Refund\n",
            &p,
        )
        .unwrap();
        assert_eq!(
            (rows[0].amount.minor(), rows[1].amount.minor()),
            (-500, 750)
        );
        let err = parse_records("d,out,in,t\n2026-10-01,,,Nothing\n", &p).unwrap_err();
        assert!(err.to_string().contains("line 2"), "{err}");
    }

    #[test]
    fn errors_name_the_line_in_the_original_file() {
        let mut p = profile(SINGLE);
        p.skip_rows = 3;
        let csv = "Datum;Betrag;Name;Zweck;Text\n01.10.2026;1,00;x;y;z\nnot a date;1,00;x;y;z\n";
        let err = parse_records(csv, &p).unwrap_err().to_string();
        assert!(
            err.starts_with("line 6:") && err.contains("not a date"),
            "{err}"
        );
    }

    #[test]
    fn missing_column_lists_what_was_found() {
        let err = parse_records("Datum;Summe\n", &profile(SINGLE))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("\"Betrag\"") && err.contains("Datum, Summe"),
            "{err}"
        );
    }

    #[test]
    fn short_rows_are_reported_not_ignored() {
        let err = parse_records(
            "Datum;Betrag;Name;Zweck;Text\n01.10.2026;1,00\n",
            &profile(SINGLE),
        )
        .unwrap_err();
        assert!(err.to_string().contains("too few columns"), "{err}");
    }
}

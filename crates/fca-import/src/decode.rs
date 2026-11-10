//! Step 1 of import: bytes to text, then drop the preamble lines.
//!
//! Must NOT know about columns or amounts.

use crate::ImportError;

/// Decode `bytes` using the encoding named `label`, then drop `skip_rows` lines.
pub fn decode(bytes: &[u8], label: &str, skip_rows: usize) -> Result<String, ImportError> {
    let encoding = encoding_rs::Encoding::for_label(label.trim().as_bytes())
        .ok_or_else(|| ImportError::UnknownEncoding(label.to_string()))?;
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(ImportError::Decode(encoding.name()));
    }
    Ok(text.split_inclusive('\n').skip(skip_rows).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_windows_1252_umlauts() {
        // "Bäckerei Müller" with ä = 0xE4 and ü = 0xFC.
        let bytes = b"B\xe4ckerei M\xfcller";
        assert_eq!(decode(bytes, "windows-1252", 0).unwrap(), "Bäckerei Müller");
    }

    #[test]
    fn skips_leading_lines() {
        assert_eq!(
            decode(b"junk\nmore junk\na;b\n1;2\n", "utf-8", 2).unwrap(),
            "a;b\n1;2\n"
        );
    }

    #[test]
    fn rejects_unknown_encoding_and_bad_bytes() {
        assert!(matches!(
            decode(b"x", "klingon", 0),
            Err(ImportError::UnknownEncoding(_))
        ));
        assert!(matches!(
            decode(b"\xff\xfe\xfd", "utf-8", 0),
            Err(ImportError::Decode(_))
        ));
    }
}

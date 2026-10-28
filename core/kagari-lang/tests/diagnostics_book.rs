#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Every diagnostic code has exactly one entry in each language of the diagnostics appendix of
//! The Kagari Reference.

use std::collections::BTreeMap;
use std::path::Path;

use kagari_lang::DiagnosticCode;

/// The code of a heading line `## E2001 …`.
fn heading_code(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("## ")?;
    let code = rest.get(..5)?;
    let mut chars = code.chars();
    let letter = chars.next()?;
    let well_formed = matches!(letter, 'E' | 'W')
        && chars.all(|c| c.is_ascii_digit())
        && rest[5..].starts_with(' ');
    well_formed.then_some(code)
}

/// The number of `## E2001 …` headings of each code in the Markdown files of `dir`.
fn documented_codes(dir: &Path) -> BTreeMap<String, usize> {
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for line in text.lines() {
            if let Some(code) = heading_code(line) {
                *found.entry(code.to_owned()).or_default() += 1;
            }
        }
    }
    found
}

#[test]
fn every_code_has_one_entry() {
    let mut expected: BTreeMap<String, usize> = BTreeMap::new();
    for code in DiagnosticCode::ALL {
        *expected.entry(code.as_str().to_owned()).or_default() += 1;
    }

    for lang in ["en", "ja"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/reference")
            .join(lang)
            .join("src/diagnostics");
        assert_eq!(documented_codes(&dir), expected, "diagnostics appendix ({lang})");
    }
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use kagari_fmt::{FormatError, format};
use kagari_syntax::SourceKind;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn source_kind(path: &Path) -> Option<SourceKind> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(SourceKind::from_extension)
}

#[test]
fn fixtures() {
    insta::glob!("fixtures/*.k{is,ix}", |path| {
        let kind = source_kind(path).unwrap();
        let input = read(path);
        let out = format(&input, kind).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
        let again = format(&out, kind).unwrap();
        assert_eq!(
            again,
            out,
            "{}: formatting is not idempotent",
            path.display()
        );
        insta::assert_snapshot!(out);
    });
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|err| panic!("{}: {err}", dir.display()));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if source_kind(&path).is_some() {
            out.push(path);
        }
    }
}

#[test]
fn corpus_is_stable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for dir in [
        "core/kagari-std/kis",
        "core/kagari-lang/tests/bytecode",
        "core/kagari-lang/tests/game_source",
        "core/kagari-syntax/tests/parse",
    ] {
        collect(&root.join(dir), &mut files);
    }
    files.sort();
    assert!(
        files.len() > 30,
        "corpus not found under {}",
        root.display()
    );
    let mut checked = 0;
    let mut failures = Vec::new();
    for path in &files {
        let kind = source_kind(path).unwrap();
        let input = read(path);
        if !kagari_syntax::parse(&input, kind).errors().is_empty() {
            continue;
        }
        checked += 1;
        match format(&input, kind) {
            Ok(out) => match format(&out, kind) {
                Ok(again) if again == out => {}
                Ok(_) => failures.push(format!("{}: not idempotent", path.display())),
                Err(err) => failures.push(format!("{}: second pass: {err}", path.display())),
            },
            Err(err) => failures.push(format!("{}: {err}", path.display())),
        }
    }
    assert!(
        checked > 30,
        "only {checked} corpus files parse without errors"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn refuses_syntax_errors() {
    let result = format("fn main( {\n", SourceKind::Kis);
    assert!(matches!(result, Err(FormatError::Syntax(errors)) if !errors.is_empty()));
}

#[test]
fn empty_file_stays_empty() {
    assert_eq!(format("", SourceKind::Kis).unwrap(), "");
    assert_eq!(format("\n\n  \n", SourceKind::Kis).unwrap(), "");
}

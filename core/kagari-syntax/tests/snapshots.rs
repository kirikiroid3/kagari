use std::fmt::Write as _;
use std::path::Path;

use kagari_syntax::{Parse, SourceKind, parse};

fn render(input: &str, parse: &Parse) -> String {
    let node = parse.syntax_node();
    assert_eq!(node.text().to_string(), input, "tree is not lossless");
    let mut out = format!("{node:#?}");
    for error in parse.errors() {
        let _ = writeln!(out, "error {:?}: {}", error.range(), error.message());
    }
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn source_kind(path: &Path) -> SourceKind {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(SourceKind::from_extension)
        .unwrap_or_else(|| panic!("{}: not a .kis or .kix file", path.display()))
}

#[test]
fn parse_fixtures() {
    insta::glob!("parse/*.k{is,ix}", |path| {
        let input = read(path);
        insta::assert_snapshot!(render(&input, &parse(&input, source_kind(path))));
    });
}

/// Every char-boundary prefix of every fixture parses to a tree with exactly that text:
/// recovery never loses input, panics or loops on truncated files.
#[test]
fn every_prefix_is_lossless() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parse");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("{}: {err}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no fixtures in {}", dir.display());
    for path in entries {
        let kind = source_kind(&path);
        let input = read(&path);
        for (end, _) in input.char_indices().chain([(input.len(), ' ')]) {
            let prefix = &input[..end];
            let parsed = parse(prefix, kind);
            assert_eq!(
                parsed.syntax_node().text().to_string(),
                prefix,
                "{}: prefix of {end} bytes is not lossless",
                path.display()
            );
        }
    }
}

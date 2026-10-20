use std::fmt::Write as _;

use kagari_syntax::{Parse, represent, script};

fn render<L: rowan::Language>(input: &str, parse: &Parse<L>) -> String {
    let node = parse.syntax_node();
    assert_eq!(node.text().to_string(), input, "tree is not lossless");
    let mut out = format!("{node:#?}");
    for error in parse.errors() {
        let _ = writeln!(out, "error {:?}: {}", error.range, error.message);
    }
    out
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

#[test]
fn kis() {
    insta::glob!("kis/*.kis", |path| {
        let input = read(path);
        insta::assert_snapshot!(render(&input, &script::parse(&input)));
    });
}

#[test]
fn kirp() {
    insta::glob!("kirp/*.kirp", |path| {
        let input = read(path);
        insta::assert_snapshot!(render(&input, &represent::parse_file(&input)));
    });
}

#[test]
fn kix() {
    insta::glob!("kirp/*.kix", |path| {
        let input = read(path);
        insta::assert_snapshot!(render(&input, &represent::parse_file(&input)));
    });
}

#[test]
fn inline_render() {
    insta::glob!("kirp/*.r", |path| {
        let input = read(path);
        insta::assert_snapshot!(render(&input, &represent::parse_inline(&input)));
    });
}

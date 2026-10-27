#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::print::print;
use super::{Doc, concat, conditional_group, group, if_break, indent, text};

fn render(mut doc: Doc) -> String {
    print(&mut doc)
}

/// `[` + `items` separated by `,` and `Line`, with a trailing comma when broken, + `]`.
fn list(items: &[&str]) -> Doc {
    let mut inner = vec![Doc::SoftLine];
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            inner.push(text(","));
            inner.push(Doc::Line);
        }
        inner.push(text(*item));
    }
    inner.push(if_break(text(","), Doc::Nil));
    group(concat(vec![
        text("["),
        indent(concat(inner)),
        Doc::SoftLine,
        text("]"),
    ]))
}

#[test]
fn group_that_fits_stays_flat() {
    assert_eq!(render(list(&["a", "b", "c"])), "[a, b, c]\n");
}

#[test]
fn group_that_does_not_fit_breaks() {
    let long = "x".repeat(60);
    let out = render(list(&[&long, &long]));
    assert_eq!(out, format!("[\n    {long},\n    {long},\n]\n"));
}

#[test]
fn if_break_follows_its_group() {
    let flat = render(list(&["a"]));
    assert_eq!(flat, "[a]\n");
    let long = "y".repeat(120);
    let broken = render(list(&[&long]));
    assert_eq!(broken, format!("[\n    {long},\n]\n"));
}

#[test]
fn repeated_hard_lines_give_one_newline() {
    let doc = concat(vec![
        text("a"),
        Doc::HardLine,
        Doc::HardLine,
        Doc::HardLine,
        text("b"),
    ]);
    assert_eq!(render(doc), "a\nb\n");
}

#[test]
fn blank_line_gives_exactly_one_empty_line() {
    let doc = concat(vec![
        Doc::BlankLine,
        text("a"),
        Doc::HardLine,
        Doc::BlankLine,
        Doc::BlankLine,
        Doc::HardLine,
        text("b"),
        Doc::BlankLine,
        text("c"),
    ]);
    assert_eq!(render(doc), "a\n\nb\n\nc\n");
}

#[test]
fn no_trailing_whitespace_and_lazy_indent() {
    let doc = concat(vec![
        text("a "),
        indent(concat(vec![Doc::HardLine, Doc::HardLine, text("b")])),
        Doc::HardLine,
        text("c"),
    ]);
    assert_eq!(render(doc), "a\n    b\nc\n");
}

#[test]
fn conditional_group_takes_hug_state_when_its_first_line_fits() {
    let body = concat(vec![
        text("{"),
        indent(concat(vec![Doc::HardLine, text("stmt;")])),
        Doc::HardLine,
        text("}"),
    ]);
    let flat = concat(vec![
        text("f("),
        text("a, "),
        group(body.clone()),
        text(")"),
    ]);
    let hug = concat(vec![
        text("f("),
        text("a, "),
        group(body.clone()),
        text(")"),
    ]);
    let broken = concat(vec![
        text("f("),
        indent(concat(vec![
            Doc::HardLine,
            text("a,"),
            Doc::HardLine,
            group(body),
            text(","),
        ])),
        Doc::HardLine,
        text(")"),
    ]);
    let out = render(conditional_group(vec![flat, hug, broken]));
    assert_eq!(out, "f(a, {\n    stmt;\n})\n");
}

#[test]
fn conditional_group_falls_back_to_last_state() {
    let long = "z".repeat(120);
    let first = text(long.clone());
    let second = text(long.clone());
    let last = text("short");
    assert_eq!(
        render(conditional_group(vec![first, second, last])),
        "short\n"
    );
}

#[test]
fn cjk_counts_two_columns_per_char() {
    let cjk = "あ".repeat(50);
    // 50 wide chars are exactly 100 columns: fits.
    let fits = group(concat(vec![text(cjk.clone()), Doc::SoftLine, text("")]));
    assert_eq!(render(fits), format!("{cjk}\n"));
    // One more column does not fit.
    let over = group(concat(vec![text(cjk.clone()), Doc::Line, text("x")]));
    assert_eq!(render(over), format!("{cjk}\nx\n"));
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{game, init_project, kagari, stderr, stdout};

#[test]
fn fresh_project_is_formatted() {
    let tmp = init_project();
    let output = kagari(&["fmt", "--check"], &game(&tmp));
    assert_eq!(output.status.code(), Some(0), "{}{}", stdout(&output), stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn fmt_rewrites_unformatted_files() {
    let tmp = init_project();
    let dir = game(&tmp);
    let path = dir.join("src/scripts/mod.kis");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\nfn  helper()->u32{1}\n");
    std::fs::write(&path, text).unwrap();

    let check = kagari(&["fmt", "--check"], &dir);
    assert_eq!(check.status.code(), Some(1));
    assert!(stdout(&check).contains("src/scripts/mod.kis"), "{}", stdout(&check));

    let format = kagari(&["fmt"], &dir);
    assert_eq!(format.status.code(), Some(0), "{}", stderr(&format));
    let formatted = std::fs::read_to_string(&path).unwrap();
    assert!(formatted.contains("fn helper() -> u32 {\n    1\n}"), "{formatted}");

    let again = kagari(&["fmt", "--check"], &dir);
    assert_eq!(again.status.code(), Some(0), "{}", stdout(&again));
}

#[test]
fn fmt_reports_syntax_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("broken.kis");
    std::fs::write(&path, "fn broken( {\n").unwrap();
    let output = kagari(&["fmt", "broken.kis"], tmp.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("cannot format"), "{}", stdout(&output));
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{game, init_project, kagari, stderr, stdout};

#[test]
fn fresh_project_has_no_diagnostics() {
    let tmp = init_project();
    let dir = game(&tmp);
    for cwd in [dir.clone(), dir.join("src/scenario")] {
        let output = kagari(&["check"], &cwd);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(stdout(&output), "no diagnostics\n");
    }
}

#[test]
fn assets_outside_the_asset_directory_are_errors() {
    let tmp = init_project();
    let dir = game(&tmp);
    let path = dir.join("src/scripts/characters.kis");
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str("\nconst BAD: ImageAsset = asset(\"../outside.png\");\n");
    std::fs::write(&path, text).unwrap();
    let output = kagari(&["check"], &dir);
    assert_eq!(output.status.code(), Some(1));
    let out = stdout(&output);
    assert!(out.contains("src/scripts/characters.kis:"), "{out}");
    assert!(out.contains("error[E1029]"), "{out}");
}

#[test]
fn missing_manifest_is_a_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let output = kagari(&["check"], tmp.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("could not find `kagari.toml`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn toolchain_mismatch_is_a_failure() {
    let tmp = init_project();
    let dir = game(&tmp);
    let path = dir.join("kagari.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let text: String = text
        .lines()
        .map(|line| {
            if line.starts_with("kagari = ") {
                "kagari = \">=99\"\n".to_owned()
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    std::fs::write(&path, text).unwrap();
    let output = kagari(&["check"], &dir);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("this project requires kagari"),
        "{}",
        stderr(&output)
    );
}

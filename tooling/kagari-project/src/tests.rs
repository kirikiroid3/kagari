#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use kagari_lang::CompileOptions;

use crate::{MANIFEST_FILE, Project, ProjectError, Target, parse_manifest};

const MINIMAL: &str = r#"
[project]
name = "game"
version = "0.1.0"

[toolchain]
kagari = "*"
"#;

#[test]
fn minimal_manifest_gets_defaults() {
    let manifest = parse_manifest(MINIMAL).unwrap();
    assert_eq!(manifest.name, "game");
    assert_eq!(manifest.version, semver::Version::new(0, 1, 0));
    assert_eq!(manifest.entry, PathBuf::from("src/main.kis"));
    assert_eq!(manifest.asset_dir, PathBuf::from("assets"));
    assert_eq!(manifest.targets, [Target::V0]);
    assert_eq!(manifest.out_dir, PathBuf::from("target"));
    assert_eq!(manifest.options, CompileOptions::default());
}

#[test]
fn unknown_keys_are_rejected() {
    let text = format!("{MINIMAL}\n[build]\nspeed = 3\n");
    let error = parse_manifest(&text).unwrap_err();
    assert!(error.contains("speed"), "{error}");
}

#[test]
fn unknown_targets_are_rejected() {
    let text = format!("{MINIMAL}\n[build]\ntargets = [\"v1\"]\n");
    assert_eq!(
        parse_manifest(&text).unwrap_err(),
        "unknown target `v1`; supported targets: v0"
    );
}

#[test]
fn duplicate_targets_are_removed() {
    let text = format!("{MINIMAL}\n[build]\ntargets = [\"v0\", \"v0\"]\n");
    assert_eq!(parse_manifest(&text).unwrap().targets, [Target::V0]);
}

#[test]
fn absolute_entries_are_rejected() {
    let text = MINIMAL.replace(
        "version = \"0.1.0\"",
        "version = \"0.1.0\"\nentry = \"/src/main.kis\"",
    );
    assert_eq!(
        parse_manifest(&text).unwrap_err(),
        "`project.entry` must be a relative path"
    );
}

#[test]
fn load_checks_the_toolchain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(MANIFEST_FILE);
    std::fs::write(&path, MINIMAL.replace("\"*\"", "\">=99\"")).unwrap();
    let error = Project::load(&path).unwrap_err();
    assert!(matches!(error, ProjectError::Toolchain { .. }), "{error}");
    assert!(
        error
            .to_string()
            .contains("this project requires kagari >=99"),
        "{error}"
    );
}

#[test]
fn find_searches_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MANIFEST_FILE), MINIMAL).unwrap();
    let nested = dir.path().join("a").join("b");
    std::fs::create_dir_all(&nested).unwrap();
    let project = Project::find(&nested).unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    assert_eq!(project.root, root);
    assert_eq!(project.manifest_path, root.join(MANIFEST_FILE));
    assert_eq!(project.out_dir(Target::V0), root.join("target").join("v0"));
}

#[test]
fn find_without_a_manifest_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b");
    std::fs::create_dir_all(&nested).unwrap();
    let error = Project::find(&nested).unwrap_err();
    assert!(matches!(error, ProjectError::NotFound { .. }), "{error}");
}

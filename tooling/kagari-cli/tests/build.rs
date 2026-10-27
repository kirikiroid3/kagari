#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::fs::File;

use common::{game, init_project, kagari, stderr};
use kagari_archive::archive::Archive;

const ASSETS: [&str; 2] = [
    "characters/character1/image/normal.png",
    "characters/character1/voice/0001.wav",
];

#[test]
fn build_writes_bytecode_and_archives() {
    let tmp = init_project();
    let dir = game(&tmp);
    let output = kagari(&["build"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let out = dir.join("target/v0");
    let bytes = std::fs::read(out.join("game.kagari")).unwrap();
    let module = kagari_bytecode::decode(&bytes).unwrap();
    kagari_bytecode::verify(&module).unwrap();
    let mut paths: Vec<&str> = module
        .assets
        .iter()
        .map(|asset| module.strings[usize::try_from(asset.path.0).unwrap()].as_str())
        .collect();
    paths.sort_unstable();
    assert_eq!(paths, ASSETS);

    let mut files: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, ["characters.kiarchive", "game.kagari"]);

    let mut archive = Archive::open(File::open(out.join("characters.kiarchive")).unwrap()).unwrap();
    for path in ASSETS {
        let expected = std::fs::read(dir.join("assets").join(path)).unwrap();
        assert_eq!(archive.read(path).unwrap(), Some(expected), "{path}");
    }

    let disasm = kagari(&["disasm", "target/v0/game.kagari"], &dir);
    assert_eq!(disasm.status.code(), Some(0), "{}", stderr(&disasm));
}

#[test]
fn build_removes_stale_archives() {
    let tmp = init_project();
    let dir = game(&tmp);
    let out = dir.join("target/v0");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("old.kiarchive"), b"stale").unwrap();
    let output = kagari(&["build"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!out.join("old.kiarchive").exists());
}

#[test]
fn unlisted_targets_are_rejected() {
    let tmp = init_project();
    let output = kagari(&["build", "--target", "v1"], &game(&tmp));
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("target `v1` is not listed in `build.targets`"),
        "{}",
        stderr(&output)
    );
}

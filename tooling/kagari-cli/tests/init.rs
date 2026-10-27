#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{game, init_project, kagari, stderr};

const OUTPUTS: &[&str] = &[
    "kagari.toml",
    ".gitignore",
    "src/main.kis",
    "src/scenario/mod.kix",
    "src/scripts/mod.kis",
    "src/scripts/characters.kis",
    "src/system/mod.kix",
    "src/effect/mod.kis",
    "assets/bgimage/.gitkeep",
    "assets/sound/.gitkeep",
    "assets/music/.gitkeep",
    "assets/characters/character1/image/normal.png",
    "assets/characters/character1/voice/0001.wav",
];

#[test]
fn init_creates_the_template() {
    let tmp = init_project();
    let dir = game(&tmp);
    for path in OUTPUTS {
        assert!(dir.join(path).is_file(), "missing {path}");
    }
    let png = std::fs::read(dir.join("assets/characters/character1/image/normal.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    let wav = std::fs::read(dir.join("assets/characters/character1/voice/0001.wav")).unwrap();
    assert!(wav.starts_with(b"RIFF"));
    let manifest = std::fs::read_to_string(dir.join("kagari.toml")).unwrap();
    assert!(manifest.contains("name = \"game\""), "{manifest}");
}

#[test]
fn init_refuses_to_overwrite() {
    let tmp = init_project();
    let output = kagari(&["init", "game"], tmp.path());
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("already exists"), "{}", stderr(&output));
}

#[test]
fn init_escapes_the_name() {
    let tmp = tempfile::tempdir().unwrap();
    let output = kagari(&["init", "game", "--name", "a\"b"], tmp.path());
    assert!(output.status.success(), "{}", stderr(&output));
    let text = std::fs::read_to_string(tmp.path().join("game/kagari.toml")).unwrap();
    let manifest = kagari_project::parse_manifest(&text).unwrap();
    assert_eq!(manifest.name, "a\"b");
}

#[cfg(not(any(windows, target_os = "macos")))]
#[test]
fn init_uses_an_installed_system_font() {
    let tmp = init_project();
    let main = std::fs::read_to_string(game(&tmp).join("src/main.kis")).unwrap();
    assert!(
        main.contains("const FONT: FontAsset = system_font(\"TestSans.ttf\");"),
        "{main}"
    );
}

#[cfg(not(any(windows, target_os = "macos")))]
#[test]
fn init_without_system_fonts_asks_for_a_font_file() {
    let tmp = tempfile::tempdir().unwrap();
    let output = common::kagari_in(&["init", "game"], tmp.path(), tmp.path());
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("put the default font at `assets/fonts/default.ttf`"),
        "{}",
        stderr(&output)
    );
    let main = std::fs::read_to_string(tmp.path().join("game/src/main.kis")).unwrap();
    assert!(
        main.contains("const FONT: FontAsset = asset(\"fonts/default.ttf\");"),
        "{main}"
    );
}

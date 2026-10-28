#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::fs::File;
use std::path::Path;

use common::{game, init_project, kagari, stderr};
use kagari_archive::archive::Archive;

const ASSETS: [&str; 2] = [
    "characters/character1/image/normal.png",
    "characters/character1/voice/0001.wav",
];

/// The bytecode path and file of the template's default font, `TestSans.ttf` of
/// [`common::font_home`]. Only the XDG platforms read `font_home`.
#[cfg(not(any(windows, target_os = "macos")))]
fn system_font() -> (&'static str, std::path::PathBuf) {
    (
        "system-fonts/TestSans.ttf",
        common::font_home().join("fonts/TestSans.ttf"),
    )
}

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
    let (fonts, mut paths): (Vec<&str>, Vec<&str>) = module
        .assets
        .iter()
        .map(|asset| module.strings[usize::try_from(asset.path.0).unwrap()].as_str())
        .partition(|path| path.starts_with("system-fonts/"));
    paths.sort_unstable();
    assert_eq!(paths, ASSETS);
    assert_eq!(fonts.len(), 1, "{fonts:?}");

    let mut files: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "characters.kiarchive",
            "game.kagari",
            "system-fonts.kiarchive"
        ]
    );

    let mut archive = Archive::open(File::open(out.join("characters.kiarchive")).unwrap()).unwrap();
    for path in ASSETS {
        let expected = std::fs::read(dir.join("assets").join(path)).unwrap();
        assert_eq!(archive.read(path).unwrap(), Some(expected), "{path}");
    }
    let mut archive =
        Archive::open(File::open(out.join("system-fonts.kiarchive")).unwrap()).unwrap();
    let font = archive.read(fonts[0]).unwrap();
    assert!(font.is_some(), "{}", fonts[0]);
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let (path, file) = system_font();
        assert_eq!(fonts[0], path);
        assert_eq!(font, Some(std::fs::read(file).unwrap()));
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

/// Declares `decl` in the template's src/main.kis and evaluates `expr` in `main`, so the bytecode
/// references the asset.
fn use_asset(dir: &Path, decl: &str, expr: &str) {
    let main = dir.join("src/main.kis");
    let source = std::fs::read_to_string(&main).unwrap();
    let marker = "fn main() -> System {";
    assert!(source.contains(marker), "{source}");
    let source = source.replace(
        marker,
        &format!("{decl}\n\nfn main() -> System {{\n    let _ = {expr};"),
    );
    std::fs::write(&main, source).unwrap();
}

/// Writes `bytes` to `assets/{path}`, makes `main` reference it, builds, and checks that the
/// build fails with `expected` on stderr and writes no bytecode.
fn rejected(path: &str, bytes: &[u8], decl: &str, expr: &str, expected: &str) {
    let tmp = init_project();
    let dir = game(&tmp);
    let file = dir.join("assets").join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, bytes).unwrap();
    use_asset(&dir, decl, expr);
    let output = kagari(&["build"], &dir);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains(expected), "{}", stderr(&output));
    assert!(!dir.join("target/v0/game.kagari").exists());
}

#[test]
fn incompatible_image_formats_fail_the_build() {
    rejected(
        "bgimage/bad.jxl",
        &[0xFF, 0x0A, 0x00, 0x00],
        "const BAD: ImageAsset = asset(\"bgimage/bad.jxl\");",
        "std::system::seen_image(BAD)",
        "error: asset `bgimage/bad.jxl` (image): JPEG XL is not supported by runtime profile `native_v0`",
    );
}

#[test]
fn incompatible_audio_formats_fail_the_build() {
    let mut ogg = b"OggS\x00\x02".to_vec();
    ogg.extend_from_slice(&[0; 20]);
    ogg.extend_from_slice(&[0x01, 0x13]);
    ogg.extend_from_slice(b"OpusHead\x01\x02\x38\x01\x80\xBB\x00\x00\x00\x00\x00");
    rejected(
        "music/theme.ogg",
        &ogg,
        "const THEME: AudioAsset = asset(\"music/theme.ogg\");",
        "std::system::heard(THEME)",
        "error: asset `music/theme.ogg` (audio): Ogg Opus is not supported by runtime profile `native_v0`",
    );
}

#[test]
fn video_assets_fail_the_build() {
    rejected(
        "movie/intro.webm",
        &[0x1A, 0x45, 0xDF, 0xA3],
        "const INTRO: VideoAsset = asset(\"movie/intro.webm\");",
        "std::system::watched(INTRO)",
        "error: asset `movie/intro.webm` (video): video assets are not supported by runtime profile `native_v0`",
    );
}

#[test]
fn invalid_shaders_fail_the_build() {
    rejected(
        "shader/broken.wgsl",
        b"alias Args = KgNoArgs;\n\nfn effect(uv: vec2<f32>) -> vec4<f32> {\n    return undefined_name;\n}\n",
        "const BROKEN: std::screen::shader::ColorShader<()> = asset(\"shader/broken.wgsl\");",
        "std::screen::shader::filter(BROKEN, ())",
        "error: shader `shader/broken.wgsl` (color): line",
    );
}

#[test]
fn std_shaders_are_packed_into_the_std_archive() {
    let tmp = init_project();
    let dir = game(&tmp);
    use_asset(&dir, "", "std::shaders::color::blur(4.0)");
    let output = kagari(&["build"], &dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let archive = dir.join("target/v0/std.kiarchive");
    assert!(archive.is_file());
    let mut archive = Archive::open(File::open(&archive).unwrap()).unwrap();
    let blur = archive.read("std/shaders/color/blur.wgsl").unwrap();
    assert!(blur.is_some_and(|text| !text.is_empty()));
}

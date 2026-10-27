//! `asset("…")` paths are relative to the program's asset directory, which may lie outside the
//! root file's directory, and may not leave it. `system_font("…")` paths are relative to the
//! system font directories, searched in order.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use kagari_lang::{CompileOptions, FilePath, MemoryFileSystem, RootDatabase, build, check};

const MAIN: &str = "/game/src/main.kix";

const SYSTEM: &str = r#"
use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

#[scene]
#[entry]
pushdown fn Start() {
    s!(voice = A, t!["Hello."]);
}

static TEXT_SPEED: Setting<Duration> = setting(ms(30));
static AUTO_DELAY: Setting<Duration> = setting(ms(1300));
static AUTO_DELAY_PER_CHAR: Setting<Duration> = setting(ms(60));
static SKIP_UNREAD: Setting<bool> = setting(false);
static MASTER: Setting<Volume> = setting(volume(100));
static MUSIC: Setting<Volume> = setting(volume(60));
static SOUND: Setting<Volume> = setting(volume(100));
static VOICE: Setting<Volume> = setting(volume(100));
static VOICE_SUSTAIN: Setting<bool> = setting(false);
static WAIT_FOR_VOICE: Setting<bool> = setting(true);
static WINDOW_OPACITY: Setting<Int<0, 100>> = setting(80);
static FULLSCREEN: Setting<bool> = setting(false);
static ANIMATIONS: Setting<bool> = setting(true);
const FONT: FontAsset = asset("font.ttf");

fn main() -> System {
    System {
        title: "Assets",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
        font: FONT,
        text: text_style(26, WHITE),
        settings: Settings {
            text_speed: TEXT_SPEED,
            auto_delay: AUTO_DELAY,
            auto_delay_per_char: AUTO_DELAY_PER_CHAR,
            skip_unread: SKIP_UNREAD,
            master: MASTER,
            music: MUSIC,
            sound: SOUND,
            voice: VOICE,
            voice_sustain: VOICE_SUSTAIN,
            wait_for_voice: WAIT_FOR_VOICE,
            window_opacity: WINDOW_OPACITY,
            fullscreen: FULLSCREEN,
            animations: ANIMATIONS,
        },
        bindings: default_bindings(),
    }
}

fn Title() -> RootPage {
    RootPage(column![
        button!{on_click = || nav::start(Start, (), |_end| ()), t!["Start"]},
    ])
}

fn GameView(game: Narrative) -> Element {
    game_screen!(game = game)
}
"#;

/// The system font directories of [`database`], in search order.
const FONT_DIRS: [&str; 2] = ["/home/user/.fonts", "/usr/share/fonts"];

fn database(asset_line: &str) -> (RootDatabase, kagari_lang::Program) {
    database_with(asset_line, CompileOptions::default())
}

fn database_with(
    asset_line: &str,
    options: CompileOptions,
) -> (RootDatabase, kagari_lang::Program) {
    database_of(format!("{SYSTEM}\n{asset_line}\n"), options)
}

fn database_of(source: String, options: CompileOptions) -> (RootDatabase, kagari_lang::Program) {
    let mut fs = MemoryFileSystem::default();
    fs.0.insert(PathBuf::from(MAIN), source);
    for file in [
        "/game/assets/bg/a.png",
        "/game/assets/font.ttf",
        "/game/assets/system-fonts/a.ttf",
        "/game/src/x.png",
        "/home/user/.fonts/Both.ttf",
        "/usr/share/fonts/Both.ttf",
        "/usr/share/fonts/truetype/Sans.ttf",
    ] {
        fs.0.insert(PathBuf::from(file), String::new());
    }
    let db = RootDatabase::new(Arc::new(fs));
    let program = db.program_with_assets(
        FilePath::disk(MAIN),
        FilePath::disk("/game/assets"),
        FONT_DIRS.iter().map(FilePath::disk).collect(),
        options,
    );
    (db, program)
}

fn codes(asset_line: &str) -> Vec<&'static str> {
    let (db, program) = database(asset_line);
    check(&db, program)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

/// `line` after the voice `A` the scene of `SYSTEM` names.
fn with_voice(line: &str) -> String {
    format!("const A: VoiceAsset = asset(\"bg/a.png\");\n{line}")
}

/// `(bytecode path, file)` of every asset of the built program, sorted.
fn built_assets(db: &RootDatabase, program: kagari_lang::Program) -> Vec<(String, FilePath)> {
    let result = build(db, program);
    assert!(
        result.diagnostics.is_empty(),
        "{:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>()
    );
    let module = result.module.expect("module");
    let mut assets: Vec<(String, FilePath)> = module
        .assets
        .iter()
        .map(|asset| module.strings[usize::try_from(asset.path.0).unwrap()].clone())
        .zip(result.asset_files)
        .collect();
    assets.sort();
    assets
}

#[test]
fn asset_paths_are_relative_to_the_asset_directory() {
    let (db, program) = database(r#"const A: VoiceAsset = asset("bg/a.png");"#);
    assert_eq!(
        built_assets(&db, program),
        [
            (
                "bg/a.png".to_owned(),
                FilePath::disk("/game/assets/bg/a.png")
            ),
            (
                "font.ttf".to_owned(),
                FilePath::disk("/game/assets/font.ttf")
            ),
        ]
    );
}

#[test]
fn system_fonts_are_found_in_the_first_directory_that_has_them() {
    let source = SYSTEM.replace("font: FONT,", "font: F,").replace(
        "text: text_style(26, WHITE),",
        "text: text_style(26, WHITE).font(G),",
    );
    let fonts = with_voice(
        r#"const F: FontAsset = system_font("truetype/Sans.ttf");
const G: FontAsset = system_font("Both.ttf");"#,
    );
    let (db, program) = database_of(format!("{source}\n{fonts}\n"), CompileOptions::default());
    let assets = built_assets(&db, program);
    assert_eq!(
        assets,
        [
            (
                "bg/a.png".to_owned(),
                FilePath::disk("/game/assets/bg/a.png")
            ),
            (
                "system-fonts/Both.ttf".to_owned(),
                FilePath::disk("/home/user/.fonts/Both.ttf")
            ),
            (
                "system-fonts/truetype/Sans.ttf".to_owned(),
                FilePath::disk("/usr/share/fonts/truetype/Sans.ttf")
            ),
        ]
    );
}

#[test]
fn system_font_errors() {
    assert_eq!(
        codes(&with_voice(
            r#"const F: FontAsset = system_font("Missing.ttf");"#
        )),
        ["E1030"]
    );
    assert_eq!(
        codes(&with_voice(
            r#"const F: FontAsset = system_font("../share/fonts/Both.ttf");"#
        )),
        ["E1029"]
    );
    assert_eq!(
        codes(&with_voice(
            r#"const F: FontAsset = system_font("/usr/share/fonts/Both.ttf");"#
        )),
        ["E1029"]
    );
    assert_eq!(
        codes(&with_voice(r#"const F: FontAsset = system_font("");"#)),
        ["E1028"]
    );
}

#[test]
fn system_fonts_can_be_disabled() {
    let options = CompileOptions {
        system_fonts: false,
        ..CompileOptions::default()
    };
    let (db, program) = database_with(
        &with_voice(r#"const F: FontAsset = system_font("Both.ttf");"#),
        options,
    );
    let codes: Vec<&str> = check(&db, program)
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert_eq!(codes, ["E1031"]);
}

#[test]
fn the_system_fonts_directory_of_the_assets_is_reserved() {
    assert_eq!(
        codes(&with_voice(
            r#"const F: FontAsset = asset("system-fonts/a.ttf");"#
        )),
        ["E1032"]
    );
}

#[test]
fn leaving_the_asset_directory_is_an_error() {
    assert_eq!(
        codes(r#"const A: VoiceAsset = asset("../src/x.png");"#),
        ["E1029"]
    );
}

#[test]
fn missing_assets_are_reported_relative_to_the_asset_directory() {
    assert_eq!(
        codes(r#"const A: VoiceAsset = asset("bg/missing.png");"#),
        ["E1027"]
    );
}

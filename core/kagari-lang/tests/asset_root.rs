//! `asset("…")` paths are relative to the program's asset directory, which may lie outside the
//! root file's directory, and may not leave it.

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

fn main() -> System {
    System {
        title: "Assets",
        resolution: size(1280, 720),
        start: Title,
        game: GameView,
        save_slots: 20,
        auto_slots: 3,
        backlog: 200,
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

fn database(asset_line: &str) -> (RootDatabase, kagari_lang::Program) {
    let mut fs = MemoryFileSystem::default();
    fs.0.insert(PathBuf::from(MAIN), format!("{SYSTEM}\n{asset_line}\n"));
    fs.0.insert(PathBuf::from("/game/assets/bg/a.png"), String::new());
    fs.0.insert(PathBuf::from("/game/src/x.png"), String::new());
    let db = RootDatabase::new(Arc::new(fs));
    let program = db.program_with_assets(
        FilePath::disk(MAIN),
        FilePath::disk("/game/assets"),
        CompileOptions::default(),
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

#[test]
fn asset_paths_are_relative_to_the_asset_directory() {
    let (db, program) = database(r#"const A: VoiceAsset = asset("bg/a.png");"#);
    let result = build(&db, program);
    assert!(
        result.diagnostics.is_empty(),
        "{:?}",
        result.diagnostics.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
    let module = result.module.expect("module");
    let paths: Vec<&str> = module
        .assets
        .iter()
        .map(|asset| module.strings[usize::try_from(asset.path.0).unwrap()].as_str())
        .collect();
    assert_eq!(paths, ["bg/a.png"]);
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

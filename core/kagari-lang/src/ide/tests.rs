#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use kagari_syntax::{TextRange, TextSize};

use super::{FileSymbol, SymbolKind, definition, file_symbols, hover};
use crate::db::{Db, MemoryFileSystem, RootDatabase};
use crate::input::{FilePath, Program, SourceFile};
use crate::pipeline::check;

const MAIN: &str = "/game/main.kix";
const OTHER: &str = "/game/other.kis";

const MAIN_SOURCE: &str = r#"mod other;

use std::narrative::text::text_style;
use std::screen::size;
use std::system::default_bindings;

/// Doc.
fn uses(screen: Screen) -> u32 {
    let x: u32 = other::helper();
    let c = Counter { n: x };
    c.get() + x
}

struct Counter {
    n: u32,
}

impl Counter {
    fn get(&self) -> u32 {
        self.n
    }
}

enum Mood {
    Calm,
    Angry,
}

mod inner {
    pub fn nested() -> u32 {
        1
    }
}

#[scene]
#[entry]
pushdown fn Start() {
    s!(t!["Hello."]);
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
        title: "Ide",
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

const OTHER_SOURCE: &str = "pub fn helper() -> u32 {\n    2\n}\n";

struct Fixture {
    db: RootDatabase,
    program: Program,
    main: SourceFile,
}

fn fixture() -> Fixture {
    let mut fs = MemoryFileSystem::default();
    fs.0.insert(PathBuf::from(MAIN), MAIN_SOURCE.to_owned());
    fs.0.insert(PathBuf::from(OTHER), OTHER_SOURCE.to_owned());
    let db = RootDatabase::new(Arc::new(fs));
    let program = db.program(FilePath::disk(MAIN));
    let main = db.source_file(FilePath::disk(MAIN));
    Fixture { db, program, main }
}

/// The offset of the first character of the `nth` (0-based) occurrence of `needle` in
/// `text`, plus `delta`.
fn offset_of(text: &str, needle: &str, nth: usize, delta: usize) -> TextSize {
    let (start, _) = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("no occurrence {nth} of `{needle}`"));
    TextSize::try_from(start.checked_add(delta).unwrap()).unwrap()
}

fn range_of(text: &str, needle: &str, nth: usize) -> TextRange {
    TextRange::at(
        offset_of(text, needle, nth, 0),
        TextSize::try_from(needle.len()).unwrap(),
    )
}

#[test]
fn fixture_program_checks_cleanly() {
    let f = fixture();
    let diagnostics = check(&f.db, f.program);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn definition_of_a_local_use_is_its_let_pattern() {
    let f = fixture();
    // `x` in `c.get() + x`.
    let offset = offset_of(MAIN_SOURCE, "+ x", 0, 2);
    let target = definition(&f.db, f.program, f.main, offset).unwrap();
    assert_eq!(target.file, FilePath::disk(MAIN));
    let pattern = offset_of(MAIN_SOURCE, "let x", 0, 4);
    assert_eq!(target.range, TextRange::at(pattern, TextSize::from(1)));
}

#[test]
fn definition_of_a_call_goes_to_the_other_file() {
    let f = fixture();
    let offset = offset_of(MAIN_SOURCE, "helper()", 0, 2);
    let target = definition(&f.db, f.program, f.main, offset).unwrap();
    assert_eq!(target.file, FilePath::disk(OTHER));
    assert_eq!(target.range, range_of(OTHER_SOURCE, "helper", 0));
}

#[test]
fn definition_of_a_method_call_goes_to_the_method_name() {
    let f = fixture();
    let offset = offset_of(MAIN_SOURCE, "c.get()", 0, 3);
    let target = definition(&f.db, f.program, f.main, offset).unwrap();
    assert_eq!(target.file, FilePath::disk(MAIN));
    assert_eq!(target.range, range_of(MAIN_SOURCE, "get", 1));
}

#[test]
fn definition_of_a_std_type_is_in_std() {
    let f = fixture();
    let offset = offset_of(MAIN_SOURCE, "Screen", 0, 1);
    let target = definition(&f.db, f.program, f.main, offset).unwrap();
    assert!(matches!(target.file, FilePath::Std(_)), "{:?}", target.file);
}

#[test]
fn hover_on_a_local_shows_its_type() {
    let f = fixture();
    let offset = offset_of(MAIN_SOURCE, "+ x", 0, 2);
    let hover = hover(&f.db, f.program, f.main, offset).unwrap();
    assert!(hover.markdown.contains("let x: u32"), "{}", hover.markdown);
    assert_eq!(hover.range, TextRange::at(offset, TextSize::from(1)));
}

#[test]
fn hover_on_a_fn_shows_its_header_and_doc() {
    let f = fixture();
    let offset = offset_of(MAIN_SOURCE, "fn uses", 0, 4);
    let hover = hover(&f.db, f.program, f.main, offset).unwrap();
    assert_eq!(
        hover.markdown,
        "```kagari\nfn uses(screen: Screen) -> u32\n```\n\nDoc."
    );
}

fn find<'a>(symbols: &'a [FileSymbol], name: &str) -> &'a FileSymbol {
    symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("no symbol `{name}` in {symbols:#?}"))
}

#[test]
fn file_symbols_nest_items() {
    let f = fixture();
    let symbols = file_symbols(&f.db, f.main);
    assert!(symbols.iter().all(|symbol| symbol.name != "nested"));

    let inner = find(&symbols, "inner");
    assert_eq!(inner.kind, SymbolKind::Module);
    let nested = find(&inner.children, "nested");
    assert_eq!(nested.kind, SymbolKind::Function);
    assert_eq!(nested.focus_range, range_of(MAIN_SOURCE, "nested", 0));

    let mood = find(&symbols, "Mood");
    assert_eq!(mood.kind, SymbolKind::Enum);
    let variants: Vec<(&str, SymbolKind)> = mood
        .children
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind))
        .collect();
    assert_eq!(
        variants,
        [
            ("Calm", SymbolKind::Variant),
            ("Angry", SymbolKind::Variant)
        ]
    );

    let imp = find(&symbols, "impl Counter");
    assert_eq!(imp.kind, SymbolKind::Impl);
    assert_eq!(find(&imp.children, "get").kind, SymbolKind::Method);

    assert_eq!(find(&symbols, "uses").kind, SymbolKind::Function);
    assert_eq!(find(&symbols, "Counter").kind, SymbolKind::Struct);
    assert_eq!(find(&symbols, "TEXT_SPEED").kind, SymbolKind::Static);
}

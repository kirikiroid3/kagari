#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use kagari_syntax::{TextRange, TextSize};

use super::{
    CompletionKind, FileEdits, FileSymbol, SymbolKind, completions, definition, file_symbols,
    hover, prepare_rename, rename,
};
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

use inner::nested as deep;

fn moods(n: u32) -> Mood {
    let c = Counter { n };
    let Counter { n: limit } = c;
    if limit > inner::nested() + deep() {
        Mood::Calm
    } else {
        Mood::Angry
    }
}

fn largest<T: Eq>(item: T) -> T {
    item
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
const FONT: FontAsset = asset("font.ttf");

fn main() -> System {
    System {
        title: "Ide",
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
    fs.0.insert(PathBuf::from("/game/font.ttf"), String::new());
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

#[test]
fn definition_of_a_path_prefix_and_a_field() {
    let f = fixture();
    let offset = offset_of(MAIN_SOURCE, "Mood::Calm", 0, 1);
    let target = definition(&f.db, f.program, f.main, offset).unwrap();
    assert_eq!(target.range, range_of(MAIN_SOURCE, "Mood", 0));

    let offset = offset_of(MAIN_SOURCE, "self.n", 0, 5);
    let target = definition(&f.db, f.program, f.main, offset).unwrap();
    let field = offset_of(MAIN_SOURCE, "n: u32,", 0, 0);
    assert_eq!(target.range, TextRange::at(field, TextSize::from(1)));
    let hover = hover(&f.db, f.program, f.main, offset).unwrap();
    assert!(hover.markdown.contains("n: u32"), "{}", hover.markdown);
}

/// `text` with `edits` (in text order, not overlapping) applied.
fn apply(text: &str, edits: &FileEdits) -> String {
    let mut out = text.to_owned();
    for edit in edits.edits.iter().rev() {
        out.replace_range(
            usize::from(edit.range.start())..usize::from(edit.range.end()),
            &edit.new_text,
        );
    }
    out
}

/// Renames what is at `offset` in `main.kix` and returns the new `main.kix` and `other.kis`.
fn renamed(f: &Fixture, offset: TextSize, new_name: &str) -> (String, String) {
    let files = rename(&f.db, f.program, f.main, offset, new_name).unwrap();
    let mut main = MAIN_SOURCE.to_owned();
    let mut other = OTHER_SOURCE.to_owned();
    for edits in &files {
        if edits.file == FilePath::disk(MAIN) {
            main = apply(MAIN_SOURCE, edits);
        } else if edits.file == FilePath::disk(OTHER) {
            other = apply(OTHER_SOURCE, edits);
        } else {
            panic!("edit outside the game: {:?}", edits.file);
        }
    }
    (main, other)
}

/// Checks that the renamed program still checks cleanly.
fn assert_checks(main: &str, other: &str) {
    let mut f = fixture();
    f.db.set_file_text(FilePath::disk(MAIN), Some(main.to_owned()));
    f.db.set_file_text(FilePath::disk(OTHER), Some(other.to_owned()));
    let diagnostics = check(&f.db, f.program);
    assert!(diagnostics.is_empty(), "{main}\n{diagnostics:#?}");
}

#[test]
fn rename_a_local_spells_out_shorthand_fields() {
    let f = fixture();
    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "n: u32) -> Mood", 0, 0), "count");
    assert!(main.contains("fn moods(count: u32) -> Mood"), "{main}");
    assert!(main.contains("Counter { n: count }"), "{main}");
    assert!(main.contains("Counter { n: limit }"), "{main}");
    assert_checks(&main, &other);
}

#[test]
fn rename_a_field_renames_accesses_literals_and_patterns() {
    let f = fixture();
    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "self.n", 0, 5), "count");
    for expected in [
        "    count: u32,\n",
        "Counter { count: x }",
        "self.count",
        "Counter { count: n }",
        "Counter { count: limit }",
    ] {
        assert!(main.contains(expected), "`{expected}` missing in\n{main}");
    }
    assert_checks(&main, &other);
}

#[test]
fn rename_items_across_files_paths_and_modules() {
    let f = fixture();
    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "Mood::Calm", 0, 0), "Feeling");
    assert!(!main.contains("Mood"), "{main}");
    assert_checks(&main, &other);

    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "mod inner", 0, 4), "outer");
    assert!(
        main.contains("mod outer {") && main.contains("outer::nested()"),
        "{main}"
    );
    assert_checks(&main, &other);

    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "helper()", 0, 0), "assist");
    assert!(main.contains("other::assist()"), "{main}");
    assert!(other.contains("pub fn assist()"), "{other}");
    assert_checks(&main, &other);

    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "fn get", 0, 3), "value");
    assert!(
        main.contains("c.value()") && main.contains("fn value(&self)"),
        "{main}"
    );
    assert_checks(&main, &other);
}

#[test]
fn rename_variants_generic_parameters_and_aliased_items() {
    let f = fixture();
    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "Calm,", 0, 0), "Serene");
    assert!(
        main.contains("Mood::Serene") && !main.contains("Calm"),
        "{main}"
    );
    assert_checks(&main, &other);

    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "T: Eq", 0, 0), "Item");
    assert!(
        main.contains("fn largest<Item: Eq>(item: Item) -> Item"),
        "{main}"
    );
    assert_checks(&main, &other);

    // An alias keeps its own name.
    let (main, other) = renamed(&f, offset_of(MAIN_SOURCE, "fn nested", 0, 3), "shallow");
    assert!(
        main.contains("use inner::shallow as deep;") && main.contains("inner::shallow() + deep()"),
        "{main}"
    );
    assert_checks(&main, &other);
    let alias = rename(
        &f.db,
        f.program,
        f.main,
        offset_of(MAIN_SOURCE, "deep()", 0, 0),
        "x",
    );
    assert!(alias.unwrap_err().0.contains("refers to `nested`"));
}

#[test]
fn rename_refusals() {
    let f = fixture();
    let prepared = prepare_rename(
        &f.db,
        f.program,
        f.main,
        offset_of(MAIN_SOURCE, "Calm", 1, 0),
    );
    assert_eq!(prepared, Ok(range_of(MAIN_SOURCE, "Calm", 1)));
    let refused = |offset: TextSize, new_name: &str| {
        rename(&f.db, f.program, f.main, offset, new_name)
            .unwrap_err()
            .0
    };
    let std_type = refused(offset_of(MAIN_SOURCE, "Screen", 0, 0), "View");
    assert!(std_type.contains("standard library"), "{std_type}");
    let file_module = refused(offset_of(MAIN_SOURCE, "other::helper", 0, 0), "elsewhere");
    assert!(file_module.contains("file"), "{file_module}");
    let keyword = refused(offset_of(MAIN_SOURCE, "+ x", 0, 2), "fn");
    assert!(keyword.contains("not a valid name"), "{keyword}");
    let nothing = refused(offset_of(MAIN_SOURCE, "{\n    let c", 0, 0), "y");
    assert!(nothing.contains("nothing"), "{nothing}");
}

#[test]
fn rename_refuses_a_name_one_use_imports_twice() {
    // `use inner::nested;` imports the module `nested` and the function `nested`.
    let mut f = fixture();
    let text = MAIN_SOURCE
        .replacen("mod inner {\n", "mod inner {\n    pub mod nested {}\n", 1)
        .replacen("mod other;\n", "mod other;\nuse inner::nested;\n", 1);
    let offset = offset_of(&text, "fn nested", 0, 3);
    f.db.set_file_text(FilePath::disk(MAIN), Some(text));
    let error = rename(&f.db, f.program, f.main, offset, "deeper")
        .unwrap_err()
        .0;
    assert!(error.contains("imports it together"), "{error}");
}

/// The completion labels at the end of the first `marker` in `main.kix` once `edited` replaces
/// `original` in it.
fn labels_at(original: &str, edited: &str, marker: &str) -> Vec<(String, CompletionKind)> {
    let mut f = fixture();
    let text = MAIN_SOURCE.replacen(original, edited, 1);
    let offset = offset_of(&text, marker, 0, marker.len());
    f.db.set_file_text(FilePath::disk(MAIN), Some(text));
    let completions = completions(&f.db, f.program, f.main, offset).unwrap();
    completions
        .items
        .into_iter()
        .map(|item| (item.label, item.kind))
        .collect()
}

fn has(labels: &[(String, CompletionKind)], label: &str, kind: CompletionKind) -> bool {
    labels.iter().any(|(l, k)| l == label && *k == kind)
}

#[test]
fn completion_after_a_dot_lists_fields_and_methods() {
    let labels = labels_at("    c.get() + x", "    c.\n    c.get() + x", "    c.\n");
    assert!(has(&labels, "n", CompletionKind::Field), "{labels:?}");
    assert!(has(&labels, "get", CompletionKind::Method), "{labels:?}");

    let labels = labels_at("    c.get() + x", "    c.ge", "c.ge");
    assert!(has(&labels, "get", CompletionKind::Method), "{labels:?}");
}

#[test]
fn completion_after_a_path_lists_its_members() {
    let labels = labels_at("        Mood::Calm", "        Mood::", "Mood::");
    assert!(has(&labels, "Calm", CompletionKind::Variant), "{labels:?}");
    assert!(has(&labels, "Angry", CompletionKind::Variant), "{labels:?}");

    let labels = labels_at("inner::nested()", "inner::ne()", "inner::ne");
    assert_eq!(labels, [("nested".to_owned(), CompletionKind::Function)]);

    let labels = labels_at(
        "use std::screen::size;",
        "use std::screen::{size, };",
        "size, ",
    );
    assert!(has(&labels, "size", CompletionKind::Function), "{labels:?}");
}

#[test]
fn completion_lists_the_names_in_scope() {
    let labels = labels_at("    c.get() + x", "    c.get() + x + s", "+ x + s");
    for (label, kind) in [
        ("x", CompletionKind::Local),
        ("c", CompletionKind::Local),
        ("screen", CompletionKind::Local),
        ("Counter", CompletionKind::Struct),
        ("other", CompletionKind::Module),
        ("std", CompletionKind::Module),
        ("u32", CompletionKind::Primitive),
    ] {
        assert!(has(&labels, label, kind), "{label} missing in {labels:?}");
    }
    assert!(
        !labels.iter().any(|(label, _)| label == "limit"),
        "{labels:?}"
    );

    let labels = labels_at(
        "let c = Counter { n: x };",
        "let c = Counter {  };",
        "Counter { ",
    );
    assert!(has(&labels, "n", CompletionKind::Field), "{labels:?}");

    // In types, only type names: after `:` and in a type path.
    for labels in [
        labels_at("let x: u32 =", "let x:  =", "let x: "),
        labels_at("-> Mood {", "-> Mo {", "-> Mo"),
    ] {
        assert!(has(&labels, "Mood", CompletionKind::Enum), "{labels:?}");
        assert!(has(&labels, "u32", CompletionKind::Primitive), "{labels:?}");
        assert!(
            !labels
                .iter()
                .any(|(label, _)| label == "moods" || label == "screen"),
            "{labels:?}"
        );
    }
    // A name being declared gets no completions.
    let mut f = fixture();
    let text = MAIN_SOURCE.replacen("let x", "let xy", 1);
    let offset = offset_of(&text, "let xy", 0, 6);
    f.db.set_file_text(FilePath::disk(MAIN), Some(text));
    assert_eq!(completions(&f.db, f.program, f.main, offset), None);
}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::db::{MemoryFileSystem, RootDatabase};
use crate::diagnostic::{DiagnosticCode, Severity};
use crate::input::FilePath;

use super::build;

#[test]
fn a_program_without_main_has_no_bytecode() {
    let mut files = FxHashMap::default();
    files.insert(
        PathBuf::from("/game/main.kis"),
        "fn helper() -> u32 { 1 }".to_owned(),
    );
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program(FilePath::disk("/game/main.kis"));
    let result = build(&db, program);
    let errors: Vec<DiagnosticCode> = result
        .diagnostics
        .iter()
        .filter(|diag| diag.severity == Severity::Error)
        .map(|diag| diag.code)
        .collect();
    assert_eq!(errors, vec![DiagnosticCode::EmitMissingMain]);
    assert!(result.module.is_none() && result.bytes.is_none());
}

/// Only functions reachable from `main` or an entry scene are emitted; a function used only
/// as the value of a constant is reachable.
#[test]
fn unreachable_functions_are_not_emitted() {
    let source = include_str!("../../tests/bytecode/005__story/main.kix").replace(
        "fn main() -> System {",
        "fn unused() -> u32 { 1 }\n\
         fn kept() -> u32 { 2 }\n\
         const PICK: fn() -> u32 = kept;\n\
         fn main() -> System {\n    let _ = PICK();",
    );
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from("/game/main.kix"), source);
    files.insert(PathBuf::from("/game/font.ttf"), String::new());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program(FilePath::disk("/game/main.kix"));
    let result = build(&db, program);
    let module = result.module.as_ref().unwrap_or_else(|| {
        panic!(
            "{}",
            crate::render_diagnostics(&db, &result.diagnostics, None)
        )
    });
    let listing = kagari_bytecode::disassemble(module, &|path: &str| path.starts_with("crate::"));
    let emitted: Vec<&str> = listing
        .lines()
        .filter_map(|line| line.strip_prefix("fn "))
        .filter_map(|line| line.split(' ').next())
        .collect();
    assert!(emitted.contains(&"crate::kept"), "{emitted:?}");
    assert!(emitted.contains(&"crate::Visit"), "{emitted:?}");
    assert!(!emitted.contains(&"crate::unused"), "{emitted:?}");
}

/// `std::ui::slider::<LO, HI>` receives its range: the import takes `props`, `LO` and `HI`.
#[test]
fn slider_import_takes_its_range() {
    let source = include_str!("../../tests/game_source/ui/001__system/main.kix").replace(
        "fn main() -> System {",
        "fn main() -> System {\n    let _ = Config;",
    );
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from("/game/main.kix"), source);
    files.insert(PathBuf::from("/game/font.ttf"), String::new());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program(FilePath::disk("/game/main.kix"));
    let result = build(&db, program);
    let module = result.module.as_ref().unwrap_or_else(|| {
        panic!(
            "{}",
            crate::render_diagnostics(&db, &result.diagnostics, None)
        )
    });
    let slider = module
        .imports
        .iter()
        .find(|import| module.strings[import.path.0 as usize] == "std::ui::slider")
        .expect("the slider import is emitted");
    assert_eq!(slider.params, 3);
    assert_eq!(kagari_bytecode::verify(module), Ok(()));
}

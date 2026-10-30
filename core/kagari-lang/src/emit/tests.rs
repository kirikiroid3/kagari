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

const HELPERS: &str = "\
fn helper(base: u32) -> u32 {
    let doubled = base + base;
    {
        let inner = doubled + 1;
        inner * 2
    }
}
fn other() -> u32 { 7 }
";

/// Builds the story fixture with `helpers` and calls of `helper` and `other` from `main`.
fn build_with_helpers(helpers: &str) -> (String, super::Build) {
    let source = include_str!("../../tests/bytecode/005__story/main.kix").replace(
        "fn main() -> System {",
        &format!("{helpers}fn main() -> System {{\n    let _ = helper(3);\n    let _ = other();"),
    );
    (source.clone(), build_source(source))
}

/// Builds `source` as `/game/main.kix`; it must have no errors.
fn build_source(source: String) -> super::Build {
    let mut files = FxHashMap::default();
    files.insert(PathBuf::from("/game/main.kix"), source);
    files.insert(PathBuf::from("/game/font.ttf"), String::new());
    let db = RootDatabase::new(Arc::new(MemoryFileSystem(files)));
    let program = db.program(FilePath::disk("/game/main.kix"));
    let result = build(&db, program);
    assert!(
        result.module.is_some(),
        "{}",
        crate::render_diagnostics(&db, &result.diagnostics, None)
    );
    result
}

fn function<'a>(debug: &'a kagari_debug::DebugInfo, path: &str) -> &'a kagari_debug::FunctionDebug {
    debug
        .functions
        .iter()
        .find(|function| function.path == path)
        .unwrap_or_else(|| panic!("no debug info for {path}"))
}

/// The debug info is parallel to the function table; scopes nest; locals name their
/// registers over the code that reads them.
#[test]
fn debug_info_describes_every_function() {
    let (source, result) = build_with_helpers(HELPERS);
    let module = result.module.as_ref().unwrap();
    let debug = result.debug.as_ref().expect("debug info with a module");
    assert_eq!(debug.functions.len(), module.functions.len());
    for (function, info) in module.functions.iter().zip(&debug.functions) {
        assert_eq!(module.strings[function.path.0 as usize], info.path);
        assert_eq!(info.body_hash.len(), 64);
        for (index, scope) in info.scopes.iter().enumerate() {
            assert!(scope.parent.is_none_or(|parent| (parent as usize) < index));
        }
    }
    let start = function(debug, "crate::Start");
    assert_eq!(start.file.as_deref(), Some("main.kix"));

    let helper = function(debug, "crate::helper");
    assert_eq!(helper.file.as_deref(), Some("main.kix"));
    assert_eq!(helper.scopes[0].parent, None);
    let names: Vec<&str> = helper
        .locals
        .iter()
        .map(|local| local.name.as_str())
        .collect();
    assert_eq!(names, ["base", "doubled", "inner"]);

    let base = &helper.locals[0];
    let code = &module
        .functions
        .iter()
        .find(|function| module.strings[function.path.0 as usize] == "crate::helper")
        .unwrap()
        .code;
    let offsets = kagari_bytecode::word_offsets(code);
    let total = *offsets.last().unwrap();
    assert_eq!(base.scope, 0);
    assert_eq!(base.ranges.len(), 1);
    assert_eq!((base.ranges[0].start, base.ranges[0].end), (0, total));

    let inner = &helper.locals[2];
    let scope = helper.scopes[inner.scope as usize];
    assert_eq!(scope.parent, Some(0));
    let text = source.find("let inner").unwrap() as u32;
    assert!(scope.start <= text && text + 9 <= scope.end, "{scope:?}");
    assert!(
        scope.end < helper.scopes[0].end,
        "the block ends before the body"
    );
    // `inner * 2` reads the register of `inner` inside one of its ranges.
    let read = code.iter().enumerate().any(|(index, instr)| {
        let kagari_bytecode::Instr::IntBin { lhs, rhs, .. } = instr else {
            return false;
        };
        inner.ranges.iter().any(|range| {
            (u32::from(*lhs) == range.register || u32::from(*rhs) == range.register)
                && !range.cell
                && range.start <= offsets[index]
                && offsets[index + 1] <= range.end
        })
    });
    assert!(read, "{inner:?} in {code:?}");
}

/// A body's hash changes with the body only: not with comments, layout or other items.
#[test]
fn body_hash_follows_the_body_only() {
    let hash = |helpers: &str| {
        let (_, result) = build_with_helpers(helpers);
        let debug = result.debug.unwrap();
        function(&debug, "crate::helper").body_hash.clone()
    };
    let original = hash(HELPERS);
    let elsewhere = HELPERS
        .replace(
            "fn helper",
            "// A comment that moves every offset.\nfn helper",
        )
        .replace("{ 7 }", "{ 8 }")
        .replace("    let doubled", "\n    let doubled");
    assert_eq!(hash(&elsewhere), original);
    let changed = HELPERS.replace("doubled + 1", "doubled + 2");
    assert_ne!(hash(&changed), original);
}

/// A step's text lives in a promoted constant, which the scene's hash covers.
#[test]
fn body_hash_covers_step_texts() {
    let hash = |from: &str, to: &str| {
        let source = include_str!("../../tests/bytecode/005__story/main.kix").replace(from, to);
        let debug = build_source(source).debug.unwrap();
        let start = function(&debug, "crate::Start").body_hash.clone();
        let visit = function(&debug, "crate::Visit").body_hash.clone();
        (start, visit)
    };
    let (start, visit) = hash("It begins.", "It begins.");
    let edited = hash("It begins.", "It starts.");
    assert_ne!(edited.0, start);
    assert_eq!(edited.1, visit);
}

/// The static types are parallel to the static table, with paths in full.
#[test]
fn debug_info_names_static_types() {
    let (_, result) = build_with_helpers(HELPERS);
    let module = result.module.as_ref().unwrap();
    let debug = result.debug.as_ref().unwrap();
    assert_eq!(debug.statics.len(), module.statics.len());
    for (s, info) in module.statics.iter().zip(&debug.statics) {
        assert_eq!(module.strings[s.path.0 as usize], info.path);
    }
    let ty = |path: &str| {
        debug
            .statics
            .iter()
            .find(|s| s.path == path)
            .map(|s| s.ty.as_str())
    };
    assert_eq!(ty("crate::VISITS"), Some("Int<0, 3>"));
    assert_eq!(ty("crate::ROUTE"), Some("crate::Route"));
    assert_eq!(ty("crate::SEEN_END"), Some("std::narrative::Meta<bool>"));
}

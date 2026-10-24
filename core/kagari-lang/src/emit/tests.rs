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

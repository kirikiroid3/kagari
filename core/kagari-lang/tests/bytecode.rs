use std::path::PathBuf;
use std::sync::Arc;

use kagari_bytecode::{decode, disassemble};
use kagari_lang::{
    Diagnostic, FilePath, OsFileSystem, RootDatabase, Severity, build, render_diagnostics,
};

/// Each fixture builds without errors to bytes that decode to the emitted module; the
/// snapshot is the warnings and the disassembly of the game's functions.
#[test]
fn bytecode() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/bytecode");
    insta::glob!("bytecode/*/main.k{is,ix}", |path| {
        let db = RootDatabase::new(Arc::new(OsFileSystem));
        let program = db.program(FilePath::disk(path));
        let result = build(&db, program);
        let errors: Vec<&Diagnostic> = result
            .diagnostics
            .iter()
            .filter(|diag| diag.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{}",
            render_diagnostics(&db, &result.diagnostics, Some(&base))
        );
        let (Some(module), Some(bytes)) = (&result.module, &result.bytes) else {
            panic!("{}: no bytecode", path.display());
        };
        assert_eq!(decode(bytes).as_ref(), Ok(module));
        let mut out = render_diagnostics(&db, &result.diagnostics, Some(&base));
        out.push_str("--- bytecode ---\n");
        // `std::narrative::text::TextEffect::and` shows the calls of the `either::<…>`
        // instances it monomorphizes (fixture `006__generics`).
        out.push_str(&disassemble(module, &|path: &str| {
            path.starts_with("crate::") || path == "std::narrative::text::TextEffect::and"
        }));
        insta::assert_snapshot!(out);
    });
}

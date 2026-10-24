use std::path::PathBuf;
use std::sync::Arc;

use kagari_lang::debug::{render_dump, resolution_dump};
use kagari_lang::{
    DiagnosticCode, FilePath, OsFileSystem, RootDatabase, Severity, build, check,
    render_diagnostics,
};

#[test]
fn game_source() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/game_source");
    insta::glob!("game_source/*/*/main.k{is,ix}", |path| {
        let db = RootDatabase::new(Arc::new(OsFileSystem));
        let program = db.program(FilePath::disk(path));
        let diags = check(&db, program);
        let mut out = render_diagnostics(&db, diags, Some(&base));
        out.push_str("--- resolutions ---\n");
        out.push_str(&resolution_dump(&db, program, Some(&base)));
        let renders = render_dump(&db, program, Some(&base));
        if !renders.is_empty() {
            out.push_str("--- renders ---\n");
            out.push_str(&renders);
        }
        insta::assert_snapshot!(out);
        // Every fixture without errors also emits verified bytecode (or has no `main`).
        if !diags.iter().any(|diag| diag.severity == Severity::Error) {
            let result = build(&db, program);
            let errors: Vec<&kagari_lang::Diagnostic> = result
                .diagnostics
                .iter()
                .filter(|diag| diag.severity == Severity::Error)
                .collect();
            assert!(
                result.bytes.is_some()
                    || errors
                        .iter()
                        .all(|diag| diag.code == DiagnosticCode::EmitMissingMain),
                "{}: {}",
                path.display(),
                render_diagnostics(&db, &result.diagnostics, Some(&base))
            );
        }
    });
}

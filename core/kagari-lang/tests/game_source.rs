use std::path::PathBuf;
use std::sync::Arc;

use kagari_lang::debug::{render_dump, resolution_dump};
use kagari_lang::{FilePath, OsFileSystem, RootDatabase, check, render_diagnostics};

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
    });
}

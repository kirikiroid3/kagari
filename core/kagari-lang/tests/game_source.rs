use std::path::PathBuf;
use std::sync::Arc;

use kagari_lang::{FilePath, OsFileSystem, RootDatabase, check, render_diagnostics};

#[test]
fn game_source() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/game_source");
    insta::glob!("game_source/script/*.kis", |path| {
        let db = RootDatabase::new(Arc::new(OsFileSystem));
        let program = db.program(FilePath::disk(path));
        let diags = check(&db, program);
        insta::assert_snapshot!(render_diagnostics(&db, diags, Some(&base)));
    });
}

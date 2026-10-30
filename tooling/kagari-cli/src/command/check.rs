//! `kagari check`: the project's diagnostics. The story analysis is reused from
//! `<out-dir>/analysis.kagari-cache` while the sources are unchanged, and the cache is
//! rewritten when the analysis runs.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::debug::resolution_dump;
use kagari_lang::{OsFileSystem, RootDatabase, Severity, check, render_diagnostics};

use crate::analysis_cache;
use crate::failure::{Failure, exit};
use crate::workspace;

pub fn run(manifest_path: Option<&Path>, resolutions: bool) -> Result<ExitCode, Failure> {
    let project = workspace::load(manifest_path)?;
    let mut db = RootDatabase::new(Arc::new(OsFileSystem));
    let program = project.program(&db);
    let cache = analysis_cache::path(&project);
    let offered = analysis_cache::offer(&mut db, program, &cache);
    let diagnostics = check(&db, program);
    analysis_cache::store(&db, program, &cache, offered);
    print!(
        "{}",
        render_diagnostics(&db, diagnostics, Some(&project.root))
    );
    if resolutions {
        print!("{}", resolution_dump(&db, program, Some(&project.root)));
    }
    Ok(exit(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error),
    ))
}

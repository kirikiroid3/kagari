//! `kagari check`: the project's diagnostics.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::debug::resolution_dump;
use kagari_lang::{OsFileSystem, RootDatabase, Severity, check, render_diagnostics};

use crate::failure::{Failure, exit};
use crate::workspace;

pub fn run(manifest_path: Option<&Path>, resolutions: bool) -> Result<ExitCode, Failure> {
    let project = workspace::load(manifest_path)?;
    let db = RootDatabase::new(Arc::new(OsFileSystem));
    let program = project.program(&db);
    let diagnostics = check(&db, program);
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

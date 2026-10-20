//! Check a Kagari program: `cargo run -p kagari-lang --example check -- <entry.kis>`.

use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::{FilePath, OsFileSystem, RootDatabase, Severity, check, render_diagnostics};

fn main() -> ExitCode {
    let Some(arg) = std::env::args_os().nth(1) else {
        eprintln!("usage: check <entry.kis>");
        return ExitCode::from(2);
    };
    let entry = match std::fs::canonicalize(&arg) {
        Ok(entry) => entry,
        Err(err) => {
            eprintln!("{}: {err}", arg.to_string_lossy());
            return ExitCode::from(2);
        }
    };
    let base = std::env::current_dir().ok();
    let db = RootDatabase::new(Arc::new(OsFileSystem));
    let program = db.program(FilePath::disk(entry));
    let diags = check(&db, program);
    print!("{}", render_diagnostics(&db, diags, base.as_deref()));
    if diags.iter().any(|diag| diag.severity == Severity::Error) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

//! Check a Kagari program:
//! `cargo run -p kagari-lang --example check -- [--resolutions] <root.kix>`.

use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::debug::resolution_dump;
use kagari_lang::{FilePath, OsFileSystem, RootDatabase, Severity, check, render_diagnostics};

fn main() -> ExitCode {
    let mut resolutions = false;
    let mut root = None;
    for arg in std::env::args_os().skip(1) {
        if arg == "--resolutions" {
            resolutions = true;
        } else if root.is_none() {
            root = Some(arg);
        } else {
            root = None;
            break;
        }
    }
    let Some(arg) = root else {
        eprintln!("usage: check [--resolutions] <root.kix>");
        return ExitCode::from(2);
    };
    let root = match std::fs::canonicalize(&arg) {
        Ok(root) => root,
        Err(err) => {
            eprintln!("{}: {err}", arg.to_string_lossy());
            return ExitCode::from(2);
        }
    };
    let base = std::env::current_dir().ok();
    let db = RootDatabase::new(Arc::new(OsFileSystem));
    let program = db.program(FilePath::disk(root));
    let diags = check(&db, program);
    print!("{}", render_diagnostics(&db, diags, base.as_deref()));
    if resolutions {
        print!("{}", resolution_dump(&db, program, base.as_deref()));
    }
    if diags.iter().any(|diag| diag.severity == Severity::Error) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

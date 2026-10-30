//! `kagari fmt`: formats the project's source files, or the given files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kagari_fmt::FormatError;
use kagari_syntax::{SourceKind, SyntaxError};

use crate::failure::{Failure, exit, io};
use crate::workspace::{self, shown};

pub fn run(
    check: bool,
    manifest_path: Option<&Path>,
    files: &[PathBuf],
) -> Result<ExitCode, Failure> {
    let files = if files.is_empty() {
        let project = workspace::load(manifest_path)?;
        let entry = project.entry_path();
        let src = entry.parent().unwrap_or(&project.root);
        let mut found = Vec::new();
        collect_sources(src, &mut found)?;
        found.sort();
        found
    } else {
        files.to_vec()
    };
    let cwd = std::env::current_dir().ok();

    let mut failed = false;
    for path in &files {
        let display = match &cwd {
            Some(cwd) if path.is_absolute() => shown(path, cwd),
            _ => path.display().to_string(),
        };
        let kind = path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(SourceKind::from_extension)
            .ok_or_else(|| Failure(format!("{display}: not a `.kis` or `.kix` file")))?;
        let text = std::fs::read_to_string(path).map_err(|error| io(path, error))?;
        match kagari_fmt::format(&text, kind) {
            Ok(formatted) if formatted == text => {}
            Ok(formatted) => {
                if check {
                    println!("would reformat {display}");
                    failed = true;
                } else {
                    std::fs::write(path, formatted).map_err(|error| io(path, error))?;
                    println!("formatted {display}");
                }
            }
            Err(FormatError::Syntax(errors)) => {
                println!("{display}: cannot format: {} syntax error(s)", errors.len());
                if let Some(error) = errors.first() {
                    let (line, col) = line_col(&text, error);
                    println!("  {line}:{col}: {}", error.message());
                }
                failed = true;
            }
            Err(FormatError::Internal(message)) => {
                println!("{display}: {message}");
                failed = true;
            }
        }
    }
    Ok(exit(failed))
}

/// Every `.kis`/`.kix` file under `dir`, skipping directories whose name starts with `.`.
fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Failure> {
    let entries = std::fs::read_dir(dir).map_err(|error| io(dir, error))?;
    for entry in entries {
        let path = entry.map_err(|error| io(dir, error))?.path();
        if path.is_dir() {
            let hidden = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if !hidden {
                collect_sources(&path, out)?;
            }
        } else if path
            .extension()
            .is_some_and(|ext| ext == "kis" || ext == "kix")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// The 1-based line and column (in chars) where `error` starts.
fn line_col(text: &str, error: &SyntaxError) -> (usize, usize) {
    let offset = usize::from(error.range().start()).min(text.len());
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count().saturating_add(1);
    let line_start = before.rfind('\n').map_or(0, |at| at.saturating_add(1));
    let col = before
        .get(line_start..)
        .map_or(0, |rest| rest.chars().count())
        .saturating_add(1);
    (line, col)
}

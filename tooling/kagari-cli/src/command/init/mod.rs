//! `kagari init`: a new project from the embedded template.

mod template;

use std::path::Path;
use std::process::ExitCode;

use crate::failure::{Failure, io};

pub fn run(path: &Path, name: Option<&str>) -> Result<ExitCode, Failure> {
    std::fs::create_dir_all(path).map_err(|error| io(path, error))?;
    let dir = std::fs::canonicalize(path).map_err(|error| io(path, error))?;
    let name = match name {
        Some(name) => name.to_owned(),
        None => dir
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or_else(|| {
                Failure(format!(
                    "{}: cannot name the project after this directory; pass `--name`",
                    dir.display()
                ))
            })?,
    };
    if name.is_empty() {
        return Err(Failure("the project name must not be empty".to_owned()));
    }
    for (rel, _) in template::FILES {
        if dir.join(rel).exists() {
            return Err(Failure(format!(
                "`{rel}` already exists in {}; `kagari init` only creates new files",
                dir.display()
            )));
        }
    }
    for (rel, contents) in template::FILES {
        let out = dir.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|error| io(parent, error))?;
        }
        std::fs::write(&out, template::render(rel, contents, &name))
            .map_err(|error| io(&out, error))?;
    }
    println!("created Kagari project `{name}` in {}", dir.display());
    Ok(ExitCode::SUCCESS)
}

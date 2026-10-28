//! `kagari build`: the project's bytecode and asset archives, per target.

mod archives;
mod compat;
mod shaders;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::{OsFileSystem, RootDatabase, build, render_diagnostics};
use kagari_project::{Project, Target};

use crate::failure::{Failure, exit, io};
use crate::workspace::{self, shown};

/// The bytecode file in each target directory.
const BYTECODE_FILE: &str = "game.kagari";

pub fn run(manifest_path: Option<&Path>, targets: &[String]) -> Result<ExitCode, Failure> {
    let project = workspace::load(manifest_path)?;
    let targets = chosen_targets(&project, targets)?;

    let db = RootDatabase::new(Arc::new(OsFileSystem));
    let result = build(&db, project.program(&db));
    if !result.diagnostics.is_empty() {
        eprint!(
            "{}",
            render_diagnostics(&db, &result.diagnostics, Some(&project.root))
        );
    }
    let (Some(module), Some(bytes)) = (result.module, result.bytes) else {
        return Ok(exit(true));
    };
    if compat::check(
        &module,
        &result.asset_files,
        &project.manifest.runtime_compatibility,
    )? > 0
    {
        return Ok(exit(true));
    }
    if shaders::check(&module, &result.asset_files, &result.shader_roles)? {
        return Ok(exit(true));
    }

    for target in targets {
        let dir = project.out_dir(target);
        std::fs::create_dir_all(&dir).map_err(|error| io(&dir, error))?;
        remove_outputs(&dir)?;
        let bytecode = dir.join(BYTECODE_FILE);
        std::fs::write(&bytecode, &bytes).map_err(|error| io(&bytecode, error))?;
        println!("wrote {}", shown(&bytecode, &project.root));
        for (archive, files) in archives::write(&module, &result.asset_files, &dir)? {
            println!("wrote {} ({files} files)", shown(&archive, &project.root));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The `--target`s, each listed in `build.targets`, or else all of `build.targets`.
fn chosen_targets(project: &Project, names: &[String]) -> Result<Vec<Target>, Failure> {
    if names.is_empty() {
        return Ok(project.manifest.targets.clone());
    }
    let mut targets = Vec::new();
    for name in names {
        let target = Target::parse(name)
            .filter(|target| project.manifest.targets.contains(target))
            .ok_or_else(|| Failure(format!("target `{name}` is not listed in `build.targets`")))?;
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

/// Deletes the outputs of an earlier build: the bytecode and every `.kiarchive`.
fn remove_outputs(dir: &Path) -> Result<(), Failure> {
    let entries = std::fs::read_dir(dir).map_err(|error| io(dir, error))?;
    for entry in entries {
        let path: PathBuf = entry.map_err(|error| io(dir, error))?.path();
        let stale = path.file_name().is_some_and(|name| name == BYTECODE_FILE)
            || path
                .extension()
                .is_some_and(|ext| ext == kagari_archive::archive::ARCHIVE_EXTENSION);
        if stale && path.is_file() {
            std::fs::remove_file(&path).map_err(|error| io(&path, error))?;
        }
    }
    Ok(())
}

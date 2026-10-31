//! `kagari build`: the project's bytecode and asset archives, per target; with the debug
//! profile also the debug artifact `game.kagari-cache`.

mod archives;
mod compat;
mod shaders;

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::cache::TOOLCHAIN_VERSION;
use kagari_lang::kagari_debug::{AnalysisCache, DEBUG_FILE, DebugArtifact, DebugInfo};
use kagari_lang::{OsFileSystem, RootDatabase, build, render_diagnostics};
use kagari_project::{Project, Target};
use sha2::{Digest, Sha256};

use crate::analysis_cache;
use crate::cli::Profile;
use crate::failure::{Failure, exit, io};
use crate::outputs::{BYTECODE_FILE, remove_target_outputs};
use crate::workspace::{self, shown};

pub fn run(
    manifest_path: Option<&Path>,
    targets: &[String],
    profile: Profile,
) -> Result<ExitCode, Failure> {
    let project = workspace::load(manifest_path)?;
    let targets = chosen_targets(&project, targets)?;
    Ok(exit(build_project(&project, &targets, profile)?))
}

/// Builds `project` for `targets` in `profile`, printing what it writes; whether it found
/// problems (diagnostics, unsupported assets, invalid shaders), in which case nothing was
/// written.
pub fn build_project(
    project: &Project,
    targets: &[Target],
    profile: Profile,
) -> Result<bool, Failure> {
    let mut db = RootDatabase::new(Arc::new(OsFileSystem));
    let program = project.program(&db);
    let cache = analysis_cache::path(project);
    let offered = analysis_cache::offer(&mut db, program, &cache);
    let result = build(&db, program);
    let analysis = analysis_cache::store(&db, program, &cache, offered);
    if !result.diagnostics.is_empty() {
        eprint!(
            "{}",
            render_diagnostics(&db, &result.diagnostics, Some(&project.root))
        );
    }
    let (Some(module), Some(bytes)) = (result.module, result.bytes) else {
        return Ok(true);
    };
    if compat::check(
        &module,
        &result.asset_files,
        &project.manifest.runtime_compatibility,
    )? > 0
    {
        return Ok(true);
    }
    if shaders::check(&module, &result.asset_files, &result.shader_roles)? {
        return Ok(true);
    }
    let artifact = match profile {
        Profile::Release => None,
        Profile::Debug => Some(debug_artifact(
            &bytes,
            analysis,
            result.debug.unwrap_or_default(),
        )?),
    };

    for &target in targets {
        let dir = project.out_dir(target);
        std::fs::create_dir_all(&dir).map_err(|error| io(&dir, error))?;
        remove_target_outputs(&dir)?;
        let bytecode = dir.join(BYTECODE_FILE);
        std::fs::write(&bytecode, &bytes).map_err(|error| io(&bytecode, error))?;
        println!("wrote {}", shown(&bytecode, &project.root));
        for (archive, files) in archives::write(&module, &result.asset_files, &dir)? {
            println!("wrote {} ({files} files)", shown(&archive, &project.root));
        }
        if let Some(artifact) = &artifact {
            let path = dir.join(DEBUG_FILE);
            std::fs::write(&path, artifact).map_err(|error| io(&path, error))?;
            println!("wrote {}", shown(&path, &project.root));
        }
    }
    Ok(false)
}

/// The bytes of the debug artifact for the bytecode `bytes`.
fn debug_artifact(
    bytes: &[u8],
    analysis: Option<AnalysisCache>,
    debug: DebugInfo,
) -> Result<Vec<u8>, Failure> {
    let (fingerprint, story) = analysis
        .map(|analysis| (analysis.fingerprint, analysis.story))
        .unwrap_or_default();
    let script_hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    DebugArtifact {
        toolchain: TOOLCHAIN_VERSION.to_owned(),
        fingerprint,
        script_hash,
        story,
        debug,
    }
    .to_bytes()
    .map_err(|error| Failure(format!("cannot encode `{DEBUG_FILE}`: {error}")))
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

//! `<out-dir>/analysis.kagari-cache`: the story analysis of the last run, reused by `check`
//! and `build` while the program's sources are unchanged.

use std::path::{Path, PathBuf};

use kagari_lang::cache::{AnalysisCacheUse, analysis_cache_use, offer_analysis_cache};
use kagari_lang::kagari_debug::{ANALYSIS_FILE, AnalysisCache};
use kagari_lang::{Program, RootDatabase};
use kagari_project::Project;

/// The cache file of `project`.
pub fn path(project: &Project) -> PathBuf {
    project.build_dir().join(ANALYSIS_FILE)
}

/// Reads the cache at `path` and offers it to the compiler. A missing, unreadable or invalid
/// file is ignored: the analysis then runs and the file is rewritten.
pub fn offer(db: &mut RootDatabase, program: Program, path: &Path) -> Option<AnalysisCache> {
    let bytes = std::fs::read(path).ok()?;
    let cache = AnalysisCache::from_bytes(bytes).ok()?;
    offer_analysis_cache(db, program, &cache).then_some(cache)
}

/// After `check` or `build`: writes the cache when the analysis ran, and returns the analysis
/// of this run (fresh, or the reused `offered` one); `None` when stage 5 did not run. A cache
/// that cannot be written is a warning, never a failure.
pub fn store(
    db: &RootDatabase,
    program: Program,
    path: &Path,
    offered: Option<AnalysisCache>,
) -> Option<AnalysisCache> {
    match analysis_cache_use(db, program) {
        AnalysisCacheUse::NotRun => None,
        AnalysisCacheUse::Reused => offered,
        AnalysisCacheUse::Fresh(cache) => {
            if let Err(message) = write(path, &cache) {
                eprintln!("kagari: warning: cannot write the analysis cache: {message}");
            }
            Some(*cache)
        }
    }
}

fn write(path: &Path, cache: &AnalysisCache) -> Result<(), String> {
    let bytes = cache.to_bytes().map_err(|error| error.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

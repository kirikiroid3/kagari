//! Reusing the story analysis (stage 5) across runs. A tool reads an earlier run's
//! `.kagari-cache` file and offers it with [`offer_analysis_cache`]; [`check`] then takes the
//! stage-5 diagnostics from it instead of analyzing again, as long as the program's
//! [`source_fingerprint`] is the one the file was written for. After `check`,
//! [`analysis_cache_use`] says whether the offer was used or what to write back.
//!
//! The fingerprint covers every input the analysis reads that stages 1–4 do not already
//! guard: the source text of every module (std included), the compile options, the asset
//! and system-font directories, and the toolchain version. Asset existence is not part of
//! it: a missing asset is a stage 1–4 error, and then stage 5 does not run. See
//! `docs/dev/src/formats/cache.md`.
//!
//! [`check`]: crate::check

use std::path::Path;
use std::sync::Arc;

use kagari_debug as kd;
use kagari_syntax::{TextRange, TextSize};
use salsa::Setter;
use sha2::{Digest, Sha256};

use crate::analysis::{analysis, story_graph};
use crate::db::{Db, RootDatabase};
use crate::def_map::{CrateId, def_maps};
use crate::diagnostic::{Diagnostic, DiagnosticCode, Related, Severity};
use crate::input::{FilePath, Program};
use crate::pipeline::analysis_runs;

/// The version written into cache files and mixed into the fingerprint.
pub const TOOLCHAIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Changes whenever the analysis may give other results for the same sources and toolchain
/// version, so that caches written before are not reused.
const FINGERPRINT_SALT: &str = "kagari-analysis/2";

/// The stage-5 diagnostics of an earlier run and the fingerprint of the sources they were
/// computed from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedAnalysis {
    fingerprint: String,
    diagnostics: Vec<Diagnostic>,
}

/// What stage 5 did for a program.
#[derive(Debug)]
pub enum AnalysisCacheUse {
    /// Stages 1–4 reported an error, so the analysis did not run.
    NotRun,
    /// The offered cache matched the sources; the analysis did not run again.
    Reused,
    /// The analysis ran; this is the cache to write for the next run.
    Fresh(Box<kd::AnalysisCache>),
}

/// The fingerprint (lowercase hex SHA-256) of every input the analysis of `program` reads.
#[salsa::tracked(returns(ref))]
pub fn source_fingerprint(db: &dyn Db, program: Program) -> String {
    let mut hasher = Sha256::new();
    let mut put = |bytes: &[u8]| {
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    put(FINGERPRINT_SALT.as_bytes());
    put(TOOLCHAIN_VERSION.as_bytes());
    let options = program.options(db);
    put(&options.state_cap.to_le_bytes());
    put(&options.recursion_depth.to_le_bytes());
    put(&[
        u8::from(options.keyframe_check),
        u8::from(options.system_fonts),
    ]);
    put(file_key(program.asset_root(db)).as_bytes());
    for dir in program.font_dirs(db).iter() {
        put(file_key(dir).as_bytes());
    }
    for &(file, krate) in &def_maps(db, program).files {
        put(file_key(file.path(db)).as_bytes());
        put(&[u8::from(krate == CrateId::Game)]);
        match file.text(db) {
            Some(text) => put(text.as_bytes()),
            None => put(&[0xff]),
        }
    }
    hex(&hasher.finalize())
}

/// A file path that tells std and disk paths apart.
fn file_key(path: &FilePath) -> String {
    match path {
        FilePath::Std(name) => format!("std:{name}"),
        FilePath::Disk(_) => format!("disk:{}", path.display(None)),
    }
}

/// Lowercase hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    out
}

/// The stage-5 diagnostics, resolved: from the offered cache when its fingerprint matches,
/// else from running the analysis.
#[salsa::tracked(returns(ref))]
pub(crate) fn analysis_diagnostics(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    if let Some(cached) = reusable(db, program) {
        return cached.diagnostics.clone();
    }
    analysis(db, program)
        .diagnostics
        .iter()
        .map(|diag| diag.resolve(db))
        .collect()
}

fn reusable(db: &dyn Db, program: Program) -> Option<&CachedAnalysis> {
    program
        .analysis_cache(db)
        .as_deref()
        .filter(|cached| cached.fingerprint == *source_fingerprint(db, program))
}

/// Offers the analysis of an earlier run. Returns `false`, and offers nothing, when the file
/// names a diagnostic code this toolchain does not know or an invalid range.
pub fn offer_analysis_cache(
    db: &mut RootDatabase,
    program: Program,
    cache: &kd::AnalysisCache,
) -> bool {
    let diagnostics: Option<Vec<Diagnostic>> = cache.diagnostics.iter().map(diagnostic).collect();
    let Some(diagnostics) = diagnostics else {
        return false;
    };
    program
        .set_analysis_cache(db)
        .to(Some(Arc::new(CachedAnalysis {
            fingerprint: cache.fingerprint.clone(),
            diagnostics,
        })));
    true
}

/// Whether stage 5 ran, was reused, or neither; with the cache to write when it ran.
pub fn analysis_cache_use(db: &dyn Db, program: Program) -> AnalysisCacheUse {
    if !analysis_runs(db, program) {
        return AnalysisCacheUse::NotRun;
    }
    if reusable(db, program).is_some() {
        return AnalysisCacheUse::Reused;
    }
    AnalysisCacheUse::Fresh(Box::new(kd::AnalysisCache {
        toolchain: TOOLCHAIN_VERSION.to_owned(),
        fingerprint: source_fingerprint(db, program).clone(),
        diagnostics: analysis_diagnostics(db, program)
            .iter()
            .map(record)
            .collect(),
        story: story_graph(db, program),
    }))
}

fn source_ref(path: &FilePath) -> kd::SourceRef {
    match path {
        FilePath::Std(name) => kd::SourceRef::Std(name.to_string()),
        FilePath::Disk(path) => kd::SourceRef::Disk(path.to_string_lossy().into_owned()),
    }
}

fn file_path(source: &kd::SourceRef) -> FilePath {
    match source {
        kd::SourceRef::Std(name) => FilePath::Std(name.as_str().into()),
        kd::SourceRef::Disk(path) => FilePath::Disk(Path::new(path).into()),
    }
}

fn range(start: u32, end: u32) -> Option<TextRange> {
    (start <= end).then(|| TextRange::new(TextSize::from(start), TextSize::from(end)))
}

fn record(diagnostic: &Diagnostic) -> kd::DiagnosticRecord {
    kd::DiagnosticRecord {
        file: source_ref(&diagnostic.file),
        start: u32::from(diagnostic.range.start()),
        end: u32::from(diagnostic.range.end()),
        severity: match diagnostic.severity {
            Severity::Error => kd::Severity::Error,
            Severity::Warning => kd::Severity::Warning,
        },
        code: diagnostic.code.as_str().to_owned(),
        message: diagnostic.message.clone(),
        related: diagnostic
            .related
            .iter()
            .map(|related| kd::RelatedRecord {
                file: source_ref(&related.file),
                start: u32::from(related.range.start()),
                end: u32::from(related.range.end()),
                message: related.message.clone(),
            })
            .collect(),
    }
}

fn diagnostic(record: &kd::DiagnosticRecord) -> Option<Diagnostic> {
    let code = DiagnosticCode::ALL
        .iter()
        .copied()
        .find(|code| code.as_str() == record.code)?;
    Some(Diagnostic {
        file: file_path(&record.file),
        range: range(record.start, record.end)?,
        severity: match record.severity {
            kd::Severity::Error => Severity::Error,
            kd::Severity::Warning => Severity::Warning,
        },
        code,
        message: record.message.clone(),
        related: record
            .related
            .iter()
            .map(|related| {
                Some(Related {
                    file: file_path(&related.file),
                    range: range(related.start, related.end)?,
                    message: related.message.clone(),
                })
            })
            .collect::<Option<_>>()?,
    })
}

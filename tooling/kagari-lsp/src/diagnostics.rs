//! Publishing diagnostics: the projects' checks plus syntax errors of open loose files,
//! computed on a background thread against a snapshot of the database.

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use crossbeam_channel::Sender;
use kagari_lang::def_map::def_maps;
use kagari_lang::{Db, FilePath, Program, RootDatabase, Severity, check};
use lsp_server::{Message, Notification};
use lsp_types::notification::{Notification as _, PublishDiagnostics};
use lsp_types::{
    DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
    PublishDiagnosticsParams,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::convert::{path_to_uri, source_kind};
use crate::line_index::LineIndex;
use crate::server::Server;

const SOURCE: &str = "kagari";

type ByPath = BTreeMap<PathBuf, Vec<lsp_types::Diagnostic>>;

/// Recomputes and publishes every diagnostic on a new thread. A later database write cancels
/// the computation; it only waits for a thread that is already publishing, so publications
/// arrive in order.
pub(crate) fn refresh(server: &Server) {
    let db = server.db.clone();
    let programs: Vec<Program> = server
        .projects
        .iter()
        .map(|loaded| loaded.program)
        .collect();
    let open: Vec<(PathBuf, String)> = server
        .open
        .iter()
        .map(|(path, text)| (path.clone(), text.clone()))
        .collect();
    let published = Arc::clone(&server.published);
    let sender = server.sender.clone();
    std::thread::spawn(move || {
        let computed = salsa::Cancelled::catch(AssertUnwindSafe(|| compute(&db, &programs, &open)));
        if let Ok(by_path) = computed {
            publish(&sender, &published, by_path);
        }
        drop(db);
    });
}

fn compute(db: &RootDatabase, programs: &[Program], open: &[(PathBuf, String)]) -> ByPath {
    let mut by_path = ByPath::new();
    let mut in_project: FxHashSet<PathBuf> = FxHashSet::default();
    let mut indexes: FxHashMap<FilePath, (Arc<str>, LineIndex)> = FxHashMap::default();
    for &program in programs {
        for (file, _) in &def_maps(db, program).files {
            if let FilePath::Disk(path) = file.path(db) {
                in_project.insert(path.to_path_buf());
            }
        }
        for diagnostic in check(db, program) {
            let FilePath::Disk(path) = &diagnostic.file else {
                continue;
            };
            let range = lsp_range(db, &mut indexes, &diagnostic.file, diagnostic.range);
            let related: Vec<DiagnosticRelatedInformation> = diagnostic
                .related
                .iter()
                .filter_map(|related| {
                    let FilePath::Disk(path) = &related.file else {
                        return None;
                    };
                    Some(DiagnosticRelatedInformation {
                        location: Location {
                            uri: path_to_uri(path)?,
                            range: lsp_range(db, &mut indexes, &related.file, related.range),
                        },
                        message: related.message.clone(),
                    })
                })
                .collect();
            let severity = match diagnostic.severity {
                Severity::Error => DiagnosticSeverity::ERROR,
                Severity::Warning => DiagnosticSeverity::WARNING,
            };
            by_path
                .entry(path.to_path_buf())
                .or_default()
                .push(lsp_types::Diagnostic {
                    range,
                    severity: Some(severity),
                    code: Some(NumberOrString::String(diagnostic.code.as_str().to_owned())),
                    source: Some(SOURCE.to_owned()),
                    message: diagnostic.message.clone(),
                    related_information: (!related.is_empty()).then_some(related),
                    ..lsp_types::Diagnostic::default()
                });
        }
    }
    for (path, text) in open {
        if in_project.contains(path) {
            continue;
        }
        let Some(kind) = source_kind(path) else {
            continue;
        };
        let parse = kagari_syntax::parse(text, kind);
        if parse.errors().is_empty() {
            continue;
        }
        let index = LineIndex::new(text);
        let diagnostics = parse.errors().iter().map(|error| lsp_types::Diagnostic {
            range: index.range(text, error.range),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some(SOURCE.to_owned()),
            message: error.message.clone(),
            ..lsp_types::Diagnostic::default()
        });
        by_path.entry(path.clone()).or_default().extend(diagnostics);
    }
    by_path
}

/// The LSP range of `range` in `file`, indexing each file's text once.
fn lsp_range(
    db: &RootDatabase,
    indexes: &mut FxHashMap<FilePath, (Arc<str>, LineIndex)>,
    file: &FilePath,
    range: kagari_syntax::TextRange,
) -> lsp_types::Range {
    let (text, index) = indexes.entry(file.clone()).or_insert_with(|| {
        let text = db
            .source_file(file.clone())
            .text(db)
            .clone()
            .unwrap_or_else(|| Arc::from(""));
        let index = LineIndex::new(&text);
        (text, index)
    });
    index.range(text, range)
}

/// Sends the diagnostics of every file that has some, and empty lists for the files that had
/// some before but none now.
fn publish(sender: &Sender<Message>, published: &Mutex<FxHashSet<PathBuf>>, by_path: ByPath) {
    let mut published = published.lock().unwrap_or_else(PoisonError::into_inner);
    let now: FxHashSet<PathBuf> = by_path.keys().cloned().collect();
    let cleared = published
        .iter()
        .filter(|path| !now.contains(*path))
        .map(|path| (path.clone(), Vec::new()));
    let sends: Vec<(PathBuf, Vec<lsp_types::Diagnostic>)> =
        by_path.into_iter().chain(cleared).collect();
    for (path, diagnostics) in sends {
        let Some(uri) = path_to_uri(&path) else {
            continue;
        };
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        };
        let notification = Notification::new(PublishDiagnostics::METHOD.to_owned(), params);
        // A closed connection ends the main loop on its own.
        let _ = sender.send(notification.into());
    }
    *published = now;
}

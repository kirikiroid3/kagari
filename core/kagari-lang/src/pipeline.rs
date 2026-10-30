//! The front-end pipeline: stages 1–5 (parse, item trees, HIR lowering, name resolution,
//! signatures and type inference, pattern and kind checks, const evaluation, analysis).

use crate::cache::analysis_diagnostics;
use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use crate::eval::const_eval;
use crate::input::Program;
use crate::item_tree::item_tree_diagnostics;
use crate::parse_query::parse;
use crate::resolve::{body_resolution, signature_resolution};
use crate::signatures::program_signatures;
use crate::typeck::exhaustive::pattern_checks;
use crate::typeck::kinds::{instant_recursion, kind_checks};

/// Every diagnostic of the program and the standard library, sorted by location and
/// deduplicated. Stage 5 runs only on a program without errors from stages 1–4: its
/// analyses execute the story and assume it is well-typed and well-kinded.
#[salsa::tracked(returns(ref))]
pub fn check(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let mut diags = front_diagnostics(db, program).clone();
    if analysis_runs(db, program) {
        diags.extend(analysis_diagnostics(db, program).iter().cloned());
    }
    diags.sort_by(|a, b| {
        (
            &a.file,
            a.range.start(),
            a.range.end(),
            a.code.as_str(),
            &a.message,
        )
            .cmp(&(
                &b.file,
                b.range.start(),
                b.range.end(),
                b.code.as_str(),
                &b.message,
            ))
    });
    diags.dedup();
    diags
}

/// Whether stage 5 runs: stages 1–4 reported no error.
pub(crate) fn analysis_runs(db: &dyn Db, program: Program) -> bool {
    !front_diagnostics(db, program)
        .iter()
        .any(|diag| diag.severity == Severity::Error)
}

/// The diagnostics of stages 1–4, unsorted.
#[salsa::tracked(returns(ref))]
fn front_diagnostics(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let maps = def_maps(db, program);
    let mut diags: Vec<Diagnostic> = maps
        .diagnostics
        .iter()
        .map(|diag| diag.resolve(db))
        .collect();
    diags.extend(
        program_signatures(db, program)
            .diagnostics
            .iter()
            .map(|diag| diag.resolve(db)),
    );
    for &(file, _) in &maps.files {
        let path = file.path(db);
        if file.text(db).is_some() {
            diags.extend(parse(db, file).errors().iter().map(|error| {
                Diagnostic::new(
                    path.clone(),
                    error.range,
                    DiagnosticCode::Syntax,
                    error.message.clone(),
                )
            }));
        }
        diags.extend(
            item_tree_diagnostics(db, file)
                .iter()
                .map(|diag| diag.resolve(db)),
        );
        for index in crate::item_tree::item_tree(db, file).indices() {
            let item = crate::item_tree::ItemId { file, index };
            let sig = &signature_resolution(db, program, item).diagnostics;
            let body = &body_resolution(db, program, item).diagnostics;
            diags.extend(sig.iter().chain(body).map(|diag| diag.resolve(db)));
        }
        for item in crate::item_tree::item_tree(db, file).indices() {
            let item = crate::item_tree::ItemId { file, index: item };
            if let Some(out) = crate::infer::infer(db, program, item) {
                diags.extend(out.diagnostics.iter().map(|diag| diag.resolve(db)));
            }
        }
        for index in crate::item_tree::item_tree(db, file).indices() {
            let item = crate::item_tree::ItemId { file, index };
            let patterns = pattern_checks(db, program, item);
            let kinds = &kind_checks(db, program, item).diagnostics;
            diags.extend(patterns.iter().chain(kinds).map(|diag| diag.resolve(db)));
        }
    }
    diags.extend(
        instant_recursion(db, program)
            .iter()
            .map(|diag| diag.resolve(db)),
    );
    diags.extend(
        const_eval(db, program)
            .diagnostics
            .iter()
            .map(|diag| diag.resolve(db)),
    );
    diags
}

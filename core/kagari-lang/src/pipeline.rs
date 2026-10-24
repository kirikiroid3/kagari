//! The front-end pipeline: stages 1–5 (parse, item trees, HIR lowering, name resolution,
//! signatures and type inference, pattern and kind checks, const evaluation, analysis).

use crate::analysis::analysis;
use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Severity};
use crate::eval::const_eval;
use crate::input::Program;
use crate::item_tree::item_tree;
use crate::parse_query::parse;
use crate::resolve::file_resolution;
use crate::signatures::program_signatures;
use crate::typeck::exhaustive::file_pattern_checks;
use crate::typeck::kinds::{file_kind_checks, instant_recursion};

/// Every diagnostic of the program and the standard library, sorted by location and
/// deduplicated. Stage 5 runs only on a program without errors from stages 1–4: its
/// analyses execute the story and assume it is well-typed and well-kinded.
#[salsa::tracked(returns(ref))]
pub fn check(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let maps = def_maps(db, program);
    let mut diags = maps.diagnostics.clone();
    diags.extend(program_signatures(db, program).diagnostics.iter().cloned());
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
            item_tree(db, file)
                .diagnostics
                .iter()
                .map(|(range, code, message)| {
                    Diagnostic::new(path.clone(), *range, *code, message.clone())
                }),
        );
        diags.extend(
            file_resolution(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
        diags.extend(
            crate::infer::file_inference(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
        diags.extend(file_pattern_checks(db, program, file).iter().cloned());
        diags.extend(
            file_kind_checks(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
    }
    diags.extend(instant_recursion(db, program).iter().cloned());
    diags.extend(const_eval(db, program).diagnostics.iter().cloned());
    if !diags.iter().any(|diag| diag.severity == Severity::Error) {
        diags.extend(analysis(db, program).diagnostics.iter().cloned());
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

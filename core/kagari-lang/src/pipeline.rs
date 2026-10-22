//! The front-end pipeline: stages 1 and 2 (parse, item trees, HIR lowering, name resolution).

use crate::db::Db;
use crate::def_map::def_maps;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hir::file_hir;
use crate::input::Program;
use crate::item_tree::item_tree;
use crate::parse_query::parse;
use crate::resolve::file_resolution;

/// Every diagnostic of the program and the standard library, sorted by location and
/// deduplicated.
#[salsa::tracked(returns(ref))]
pub fn check(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let maps = def_maps(db, program);
    let mut diags = maps.diagnostics.clone();
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
        let local = item_tree(db, file)
            .diagnostics
            .iter()
            .chain(&file_hir(db, file).diagnostics);
        diags.extend(local.map(|(range, code, message)| {
            Diagnostic::new(path.clone(), *range, *code, message.clone())
        }));
        diags.extend(
            file_resolution(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
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

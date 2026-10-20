//! The front-end pipeline: stages 1-3 plus the termination and missing-return checks.

use rustc_hash::FxHashSet;

use crate::collect::collect;
use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::input::{Program, SourceFile};
use crate::ir::returns::missing_returns;
use crate::ir::termination::termination;
use crate::kis::game_hir;
use crate::kis::hir::Render;
use crate::parse::{parse_kis, parse_represent};
use crate::represent::check::{
    check_cycles, check_inline_renders, check_kirp_file, check_kix_file, check_voices,
};
use crate::represent::scope::item_scope;

/// Every diagnostic of the program, sorted by location and deduplicated.
#[salsa::tracked(returns(ref))]
pub fn check(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let collected = collect(db, program);
    let hir = game_hir(db, program);
    let kirp_files: Vec<SourceFile> = {
        let mut seen = FxHashSet::default();
        hir.renders
            .iter()
            .filter_map(|(_, render)| match render {
                Render::File { kirp, .. } => Some(*kirp),
                Render::Inline { .. } => None,
            })
            .filter(|file| seen.insert(*file))
            .collect()
    };

    let mut diags = collected.diagnostics.clone();

    for &file in &collected.kis_files {
        syntax_errors(db, file, parse_kis(db, file).errors(), &mut diags);
    }
    for &file in collected.kix_files.iter().chain(&kirp_files) {
        syntax_errors(db, file, parse_represent(db, file).errors(), &mut diags);
    }

    diags.extend(hir.diagnostics.iter().cloned());
    diags.extend(termination(db, program).iter().cloned());
    diags.extend(missing_returns(db, program).iter().cloned());

    diags.extend(item_scope(db, program).diagnostics.iter().cloned());
    for &file in &collected.kix_files {
        diags.extend(
            check_kix_file(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
    }
    for &file in &kirp_files {
        diags.extend(
            check_kirp_file(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
    }
    for &file in &collected.kis_files {
        diags.extend(
            check_inline_renders(db, program, file)
                .diagnostics
                .iter()
                .cloned(),
        );
    }
    diags.extend(check_voices(db, program).iter().cloned());
    diags.extend(check_cycles(db, program).iter().cloned());

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

fn syntax_errors(
    db: &dyn Db,
    file: SourceFile,
    errors: &[kagari_syntax::SyntaxError],
    out: &mut Vec<Diagnostic>,
) {
    if file.text(db).is_none() {
        return;
    }
    let path = file.path(db);
    out.extend(errors.iter().map(|error| {
        Diagnostic::new(
            path.clone(),
            error.range,
            DiagnosticCode::Syntax,
            error.message.clone(),
        )
    }));
}

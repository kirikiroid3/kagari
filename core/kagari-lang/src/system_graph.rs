//! The mode-graph checks of the system scope: reachability, context typing, game-top
//! operations, popping the base, trapped overlays and unbounded push cycles.

use la_arena::Idx;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::db::Db;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Span};
use crate::input::Program;
use crate::kis::program_hir;
use crate::kis::system_hir::{
    ModeDecl, ModeKind, SysFnDecl, SysOp, SysStmt, SysStmtKind, SystemHir, UiKind, UiNode,
};
use crate::represent::check::tarjan;

/// The mode can be shown with no game running.
const NO_GAME: u8 = 1;
/// The mode can be shown while a game runs.
const GAME: u8 = 2;

#[salsa::tracked(returns(ref))]
pub fn system_graph(db: &dyn Db, program: Program) -> Vec<Diagnostic> {
    let system = &program_hir(db, program).system;
    let Some(start) = system.start else {
        return Vec::new();
    };
    let mut graph = Graph {
        db,
        system,
        memo: FxHashMap::default(),
        visiting: FxHashSet::default(),
        out: Vec::new(),
    };
    graph.check(start);
    graph.out
}

/// What a mode does: an operation, or a repetition that reads the running game.
#[derive(Clone, Copy)]
enum Use {
    Op(SysOp),
    ForHistory,
    ForChoices,
}

#[derive(Clone, Copy)]
struct OpUse {
    what: Use,
    /// The handler attribute, or the statement in `<on-exit>`.
    site: Span,
    /// The operation itself, inside a system function when reached through a call.
    origin: Span,
}

struct Graph<'a> {
    db: &'a dyn Db,
    system: &'a SystemHir,
    memo: FxHashMap<Idx<SysFnDecl>, Vec<(SysOp, Span)>>,
    visiting: FxHashSet<Idx<SysFnDecl>>,
    out: Vec<Diagnostic>,
}

fn index<T>(idx: Idx<T>) -> usize {
    usize::try_from(u32::from(idx.into_raw())).unwrap_or(usize::MAX)
}

impl Graph<'_> {
    fn check(&mut self, start: Idx<ModeDecl>) {
        let system = self.system;
        let modes: Vec<(Idx<ModeDecl>, &ModeDecl)> = system.modes.iter().collect();
        let ops: Vec<Vec<OpUse>> = modes.iter().map(|(_, mode)| self.mode_ops(mode)).collect();
        let count = modes.len();

        // Contexts, propagated from the start page along the edges of reachable modes.
        let mut ctx = vec![0u8; count];
        if let Some(slot) = ctx.get_mut(index(start)) {
            *slot = NO_GAME;
        }
        let game = system.game_mode.map(index);
        let mut changed = true;
        while changed {
            changed = false;
            for (from, uses) in ops.iter().enumerate() {
                let here = ctx.get(from).copied().unwrap_or(0);
                if here == 0 {
                    continue;
                }
                for op_use in uses {
                    let (target, add) = match op_use.what {
                        Use::Op(SysOp::Goto(mode)) => (Some(index(mode)), NO_GAME),
                        Use::Op(SysOp::Start(_) | SysOp::Load(_)) => (game, GAME),
                        Use::Op(SysOp::Push(mode)) => (Some(index(mode)), here),
                        _ => (None, 0),
                    };
                    if let Some(slot) = target.and_then(|target| ctx.get_mut(target))
                        && *slot | add != *slot
                    {
                        *slot |= add;
                        changed = true;
                    }
                }
            }
        }

        // Resets(M): M can leave its frame without popping it.
        let mut resets: Vec<bool> = ops
            .iter()
            .map(|uses| {
                uses.iter().any(|op_use| {
                    matches!(
                        op_use.what,
                        Use::Op(SysOp::Goto(_) | SysOp::Start(_) | SysOp::Load(_) | SysOp::Quit)
                    )
                })
            })
            .collect();
        let mut changed = true;
        while changed {
            changed = false;
            for (from, uses) in ops.iter().enumerate() {
                if resets.get(from).copied().unwrap_or(true) {
                    continue;
                }
                let pushes_reset = uses.iter().any(|op_use| match op_use.what {
                    Use::Op(SysOp::Push(mode)) => resets.get(index(mode)).copied().unwrap_or(false),
                    _ => false,
                });
                if pushes_reset && let Some(slot) = resets.get_mut(from) {
                    *slot = true;
                    changed = true;
                }
            }
        }

        for (position, ((idx, mode), uses)) in modes.iter().zip(&ops).enumerate() {
            let here = ctx.get(position).copied().unwrap_or(0);
            if here == 0 {
                self.push(
                    mode.span,
                    DiagnosticCode::KisUnreachableMode,
                    format!("mode `{}` can never be shown", mode.name),
                    None,
                );
            }
            let is_base = matches!(mode.kind, ModeKind::RootPage | ModeKind::Game);
            let mut pops = false;
            for op_use in uses {
                if here & NO_GAME != 0
                    && let Some(what) = self.needs_game(op_use.what)
                {
                    self.push(
                        op_use.site,
                        DiagnosticCode::KisContextRequires,
                        format!(
                            "{what} needs a running game, but `{}` can be shown without one",
                            mode.name
                        ),
                        Some(op_use),
                    );
                }
                let op = match op_use.what {
                    Use::Op(op) => op,
                    Use::ForHistory | Use::ForChoices => continue,
                };
                match op {
                    SysOp::Advance | SysOp::Select(_) if Some(*idx) != system.game_mode => {
                        self.push(
                            op_use.site,
                            DiagnosticCode::KisNeedsGameMode,
                            format!(
                                "`{}` acts on the running story, but `{}` is not the game mode",
                                op_name(op),
                                mode.name
                            ),
                            Some(op_use),
                        );
                    }
                    SysOp::Pop | SysOp::Revert => {
                        pops = true;
                        if is_base {
                            self.push(
                                op_use.site,
                                DiagnosticCode::KisPopFromBase,
                                format!(
                                    "`{}` would pop `{}`, the base of the stack",
                                    op_name(op),
                                    mode.name
                                ),
                                Some(op_use),
                            );
                        }
                    }
                    _ => {}
                }
            }
            if here != 0 && !is_base && !pops && !resets.get(position).copied().unwrap_or(true) {
                self.push(
                    mode.span,
                    DiagnosticCode::KisTrappedMode,
                    format!(
                        "mode `{}` can never be left: it has no `interface__return` and reaches no `interface__goto`, `metafn__start`, `metafn__load` or `metafn__quit`",
                        mode.name
                    ),
                    None,
                );
            }
        }

        // Push cycles among reachable modes.
        let reachable = |mode: usize| ctx.get(mode).copied().unwrap_or(0) != 0;
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut sites: Vec<(usize, usize, Span)> = Vec::new();
        for (from, uses) in ops.iter().enumerate() {
            if !reachable(from) {
                continue;
            }
            for op_use in uses {
                if let Use::Op(SysOp::Push(mode)) = op_use.what
                    && reachable(index(mode))
                    && let Some(successors) = adjacency.get_mut(from)
                {
                    successors.push(index(mode));
                    sites.push((from, index(mode), op_use.site));
                }
            }
        }
        let db = self.db;
        for component in tarjan(&adjacency) {
            let site = sites
                .iter()
                .filter(|(from, to, _)| component.contains(from) && component.contains(to))
                .map(|(_, _, span)| *span)
                .min_by(|a, b| {
                    (a.file.path(db), a.range.start()).cmp(&(b.file.path(db), b.range.start()))
                });
            let Some(site) = site else {
                continue;
            };
            let mut names: Vec<String> = component
                .iter()
                .filter_map(|position| modes.get(*position))
                .map(|(_, mode)| format!("`{}`", mode.name))
                .collect();
            names.sort();
            self.push(
                site,
                DiagnosticCode::KisPushCycle,
                format!(
                    "modes push each other in a cycle, so the stack can grow without bound: {}",
                    names.join(", ")
                ),
                None,
            );
        }
    }

    /// What `what` is called in a context message, when it needs a running game.
    fn needs_game(&self, what: Use) -> Option<String> {
        match what {
            Use::Op(SysOp::Push(mode)) if self.system.modes[mode].requires_game => Some(format!(
                "`interface__call({})`",
                self.system.modes[mode].name
            )),
            Use::Op(op @ (SysOp::Save(_) | SysOp::Revert | SysOp::PlayVoice)) => {
                Some(format!("`{}`", op_name(op)))
            }
            Use::ForHistory => Some("`<for-history>`".to_owned()),
            Use::ForChoices => Some("`<for-choices>`".to_owned()),
            Use::Op(_) => None,
        }
    }

    fn push(&mut self, span: Span, code: DiagnosticCode, message: String, op_use: Option<&OpUse>) {
        let mut diag = span.diagnostic(self.db, code, message);
        if let Some(op_use) = op_use
            && op_use.origin != op_use.site
        {
            diag.related
                .push(op_use.origin.related(self.db, "operation here"));
        }
        self.out.push(diag);
    }

    // ---- operation collection -------------------------------------------------------------

    /// Every operation a mode can run, through its UI, input handlers and `<on-exit>`.
    fn mode_ops(&mut self, mode: &ModeDecl) -> Vec<OpUse> {
        let mut out = Vec::new();
        self.ui_ops(&mode.ui, &mut out);
        for handler in mode.on_keyboard.iter().chain(&mode.on_controller) {
            self.stmt_ops(handler, handler.span, &mut out);
        }
        for arm in &mode.on_exit {
            self.on_exit_ops(&arm.body, &mut out);
        }
        out
    }

    fn ui_ops(&mut self, nodes: &[UiNode], out: &mut Vec<OpUse>) {
        for node in nodes {
            let repetition = match node.kind {
                UiKind::ForHistory => Some(Use::ForHistory),
                UiKind::ForChoices => Some(Use::ForChoices),
                _ => None,
            };
            if let Some(what) = repetition {
                out.push(OpUse {
                    what,
                    site: node.span,
                    origin: node.span,
                });
            }
            if let Some(handler) = &node.on_click {
                self.stmt_ops(handler, handler.span, out);
            }
            self.ui_ops(&node.children, out);
        }
    }

    /// `<on-exit>` statements are their own sites.
    fn on_exit_ops(&mut self, stmts: &[SysStmt], out: &mut Vec<OpUse>) {
        for stmt in stmts {
            match &stmt.kind {
                SysStmtKind::Match(m) => {
                    for arm in &m.arms {
                        self.on_exit_ops(&arm.body, out);
                    }
                }
                _ => self.stmt_ops(stmt, stmt.span, out),
            }
        }
    }

    fn stmt_ops(&mut self, stmt: &SysStmt, site: Span, out: &mut Vec<OpUse>) {
        match &stmt.kind {
            SysStmtKind::Op(op) => out.push(OpUse {
                what: Use::Op(*op),
                site,
                origin: stmt.span,
            }),
            SysStmtKind::Call(call) => {
                for (op, origin) in self.sysfn_ops(call.func) {
                    out.push(OpUse {
                        what: Use::Op(op),
                        site,
                        origin,
                    });
                }
            }
            SysStmtKind::Match(m) => {
                for arm in &m.arms {
                    for inner in &arm.body {
                        self.stmt_ops(inner, site, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// Every operation a system function can run, over all branches and calls.
    fn sysfn_ops(&mut self, func: Idx<SysFnDecl>) -> Vec<(SysOp, Span)> {
        if let Some(ops) = self.memo.get(&func) {
            return ops.clone();
        }
        // Recursion is reported by the lowering; cut it here.
        if !self.visiting.insert(func) {
            return Vec::new();
        }
        let system = self.system;
        let mut ops = Vec::new();
        self.body_ops(&system.sysfns[func].body, &mut ops);
        self.visiting.remove(&func);
        self.memo.insert(func, ops.clone());
        ops
    }

    fn body_ops(&mut self, stmts: &[SysStmt], out: &mut Vec<(SysOp, Span)>) {
        for stmt in stmts {
            match &stmt.kind {
                SysStmtKind::Op(op) => out.push((*op, stmt.span)),
                SysStmtKind::Call(call) => out.extend(self.sysfn_ops(call.func)),
                SysStmtKind::Match(m) => {
                    for arm in &m.arms {
                        self.body_ops(&arm.body, out);
                    }
                }
                _ => {}
            }
        }
    }
}

fn op_name(op: SysOp) -> &'static str {
    match op {
        SysOp::Start(_) => "metafn__start",
        SysOp::Load(_) => "metafn__load",
        SysOp::Save(_) => "metafn__save",
        SysOp::Advance => "metafn__advance",
        SysOp::Select(_) => "metafn__select",
        SysOp::Quit => "metafn__quit",
        SysOp::Revert => "metafn__revert",
        SysOp::PlayVoice => "metafn__play_voice",
        SysOp::Push(_) => "interface__call",
        SysOp::Pop => "interface__return",
        SysOp::Goto(_) => "interface__goto",
        SysOp::FocusMove(_) => "interface__focus_move",
        SysOp::FocusActivate => "interface__focus_activate",
    }
}

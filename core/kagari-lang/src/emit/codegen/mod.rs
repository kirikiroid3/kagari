//! Bytecode for one instance, emitted directly from its MIR body: blocks laid out in
//! reverse postorder, registers from [`regalloc`], then statements and terminators one by
//! one. Constructors and builtins used as values have no MIR: their wrappers are emitted
//! directly (`call.rs`).

mod call;
mod place;
mod regalloc;
mod rvalue;
mod term;

use kagari_bytecode::{Instr, Label, Reg};

use crate::diagnostic::{Anchor, Diagnostic, NodeId, Owner, resolve_anchor};
use crate::eval::machine::value::GenericEnv;
use crate::eval::machine::{ErrorKind, EvalError};
use crate::hir::source_map::StoreSourceMap;
use crate::hir::{ExprId, body_source_map_of};
use crate::mir::{BasicBlock, Body, Statement, StatementKind, item_mir};
use crate::ty::FnDefId;

use super::{Emitter, Instance};
use regalloc::Frame;

/// A lowered function.
pub(super) struct Lowered {
    pub code: Vec<Instr>,
    pub regs: u16,
    /// More registers than a function can have.
    pub too_large: bool,
    /// `(instruction index, byte offset in the source file)`.
    pub lines: Vec<(u32, u32)>,
}

/// A register while lowering; converted to a `Reg` when an instruction is made.
type R = u32;

fn reg(r: R) -> Reg {
    u16::try_from(r).unwrap_or(u16::MAX)
}

fn count(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn nth(first: R, index: usize) -> R {
    first.saturating_add(u32::try_from(index).unwrap_or(u32::MAX))
}

/// Lowers one instance of the module.
pub(super) fn lower_instance(em: &mut Emitter<'_>, instance: &Instance) -> Lowered {
    match instance {
        Instance::Ctor(fn_ref) => call::ctor_body(em, fn_ref),
        Instance::Builtin(fn_ref) => call::builtin_body(em, instance, fn_ref),
        Instance::Fn(_)
        | Instance::Closure { .. }
        | Instance::ConstInit(..)
        | Instance::Promoted(..) => match mir_body(em, instance) {
            Some(body) => FnCodegen::new(em, instance, body).run(),
            None => Lowered {
                code: vec![Instr::Unreachable],
                regs: 0,
                too_large: false,
                lines: Vec::new(),
            },
        },
    }
}

/// The MIR body of an instance with one.
fn mir_body<'db>(em: &Emitter<'db>, instance: &Instance) -> Option<&'db Body> {
    let item = em.instance_item(instance)?;
    let mir = item_mir(em.db, em.program, item).as_ref()?;
    match instance {
        Instance::Fn(fn_ref) => match fn_ref.def {
            FnDefId::Item(_) => Some(&mir.main),
            _ => None,
        },
        Instance::ConstInit(..) => Some(&mir.main),
        Instance::Closure { expr, .. } => mir.closure(*expr),
        Instance::Promoted(_, index) => mir.promoted(*index),
        Instance::Ctor(_) | Instance::Builtin(_) => None,
    }
}

/// The emitter of one function.
struct FnCodegen<'e, 'db> {
    em: &'e mut Emitter<'db>,
    /// `None` for the wrappers without MIR.
    body: Option<&'db Body>,
    env: GenericEnv,
    /// The instance that owns the closures and promoted constants this function uses.
    root: Instance,
    frame: Frame,
    /// The blocks in emission order.
    layout: Vec<BasicBlock>,
    code: Vec<Instr>,
    /// Per label, the instruction it is bound to. Labels `0..blocks` are the blocks.
    labels: Vec<Option<u32>>,
    /// The next free scratch register; reset after every statement.
    next: R,
    high: R,
    lines: Vec<(u32, u32)>,
    source_map: Option<&'db StoreSourceMap>,
    /// The HIR expression of the statement or terminator being emitted.
    src: Option<ExprId>,
    /// Its source offset, until an instruction records it.
    pending: Option<u32>,
}

impl<'e, 'db> FnCodegen<'e, 'db> {
    fn new(em: &'e mut Emitter<'db>, instance: &Instance, body: &'db Body) -> Self {
        let layout = regalloc::layout(body);
        let frame = regalloc::allocate(body, &layout);
        let env = em.instance_env(instance);
        let root = match instance {
            Instance::Closure { owner, .. } | Instance::Promoted(owner, _) => (**owner).clone(),
            _ => instance.clone(),
        };
        let source_map = body_source_map_of(em.db, body.source.owner());
        let mut labels = Vec::new();
        labels.resize(body.blocks.len(), None);
        let next = frame.scratch;
        FnCodegen {
            em,
            body: Some(body),
            env,
            root,
            frame,
            layout,
            code: Vec::new(),
            labels,
            next,
            high: next,
            lines: Vec::new(),
            source_map,
            src: None,
            pending: None,
        }
    }

    /// A function without MIR whose scratch registers start at `scratch`.
    fn bare(em: &'e mut Emitter<'db>, instance: &Instance, scratch: R) -> Self {
        let env = em.instance_env(instance);
        FnCodegen {
            em,
            body: None,
            env,
            root: instance.clone(),
            frame: Frame {
                locs: Vec::new(),
                cells: Vec::new(),
                window: scratch,
                scratch,
            },
            layout: Vec::new(),
            code: Vec::new(),
            labels: Vec::new(),
            next: scratch,
            high: scratch,
            lines: Vec::new(),
            source_map: None,
            src: None,
            pending: None,
        }
    }

    fn run(mut self) -> Lowered {
        let Some(body) = self.body else {
            self.emit(Instr::Unreachable);
            return self.finish();
        };
        let layout = std::mem::take(&mut self.layout);
        self.prologue();
        for (position, block) in layout.iter().enumerate() {
            let Some(data) = body.block(*block) else {
                continue;
            };
            self.bind(block_label(*block));
            for stmt in &data.stmts {
                self.set_src(stmt.src);
                self.statement(stmt);
                self.next = self.frame.scratch;
            }
            self.set_src(data.term_src);
            self.terminator(&data.term, layout.get(position.saturating_add(1)).copied());
            self.next = self.frame.scratch;
        }
        self.finish()
    }

    /// Creates the cells of the celled locals.
    fn prologue(&mut self) {
        let cells = self.frame.cells.clone();
        for (cell, init) in cells {
            // Registers start uninitialized, so a fresh scratch register is `Uninit`.
            let init = match init {
                Some(init) => init,
                None => self.frame.scratch,
            };
            self.high = self.high.max(self.frame.scratch.saturating_add(1));
            self.emit(Instr::Cell {
                dst: reg(cell),
                init: reg(init),
            });
        }
    }

    fn statement(&mut self, stmt: &Statement) {
        match &stmt.kind {
            StatementKind::Assign(place, rvalue) => self.assign(place, rvalue),
            StatementKind::Call { dest, callee, args } => self.call_stmt(dest, callee, args),
        }
    }

    // ----- Emission -----------------------------------------------------------------

    fn set_src(&mut self, src: Option<ExprId>) {
        self.src = src;
        self.pending = match (src, self.source_map) {
            (Some(expr), Some(map)) => Some(u32::from(map.expr(expr).start())),
            _ => None,
        };
    }

    fn emit(&mut self, instr: Instr) {
        if let Some(offset) = self.pending.take()
            && self.lines.last().is_none_or(|(_, last)| *last != offset)
        {
            self.lines
                .push((u32::try_from(self.code.len()).unwrap_or(u32::MAX), offset));
        }
        self.code.push(instr);
    }

    fn bind(&mut self, label: Label) {
        let here = u32::try_from(self.code.len()).unwrap_or(u32::MAX);
        if let Some(slot) = self.labels.get_mut(label.0 as usize) {
            *slot = Some(here);
        }
    }

    fn jmp(&mut self, target: BasicBlock) {
        self.emit(Instr::Jmp {
            target: block_label(target),
        });
    }

    /// A fresh scratch register.
    fn temp(&mut self) -> R {
        let r = self.next;
        self.next = self.next.saturating_add(1);
        self.high = self.high.max(self.next);
        r
    }

    fn mov(&mut self, dst: R, src: R) {
        if dst != src {
            self.emit(Instr::Move {
                dst: reg(dst),
                src: reg(src),
            });
        }
    }

    fn finish(self) -> Lowered {
        let labels = self.labels;
        let target =
            |label: Label| Label(labels.get(label.0 as usize).copied().flatten().unwrap_or(0));
        let code = self
            .code
            .into_iter()
            .map(|instr| match instr {
                Instr::Jmp { target: label } => Instr::Jmp {
                    target: target(label),
                },
                Instr::JmpIf {
                    cond,
                    target: label,
                } => Instr::JmpIf {
                    cond,
                    target: target(label),
                },
                Instr::JmpNot {
                    cond,
                    target: label,
                } => Instr::JmpNot {
                    cond,
                    target: target(label),
                },
                instr => instr,
            })
            .collect();
        Lowered {
            code,
            regs: reg(self.high),
            too_large: self.high > u32::from(u16::MAX),
            lines: self.lines,
        }
    }

    // ----- Errors -------------------------------------------------------------------

    /// The anchor of the statement or terminator being emitted.
    fn anchor(&self) -> Option<Anchor> {
        let owner = self.body?.source.owner();
        Some(Anchor::Node(Owner::Body(owner), NodeId::Expr(self.src?)))
    }

    /// A const-evaluation fault found while emitting (a generic constant instantiated only
    /// by run-time code).
    fn eval_error(&mut self, error: &EvalError) {
        let ErrorKind::Fault(fault) = &error.kind else {
            return;
        };
        let Some(anchor) = error.at.or_else(|| self.anchor()) else {
            return;
        };
        let (file, range) = resolve_anchor(self.em.db, &anchor);
        self.em.diagnostics.push(Diagnostic::new(
            file.path(self.em.db).clone(),
            range,
            fault.code,
            fault.message.clone(),
        ));
    }

    /// A violated MIR invariant: the function panics there.
    fn internal_error(&mut self, message: &str) {
        if let Some(item) = self.em.instance_item(&self.root) {
            self.em.internal_error(item, message);
        }
        self.emit(Instr::Unreachable);
    }
}

/// The label of a block: its arena index.
fn block_label(block: BasicBlock) -> Label {
    Label(block.into_raw().into_u32())
}

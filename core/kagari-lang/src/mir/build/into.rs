//! Expressions written into a destination: blocks, `if`, `&&`/`||`, assignments, and the
//! dispatch to the other lowerings.

use crate::eval::ConstValue;
use crate::hir::BinaryOp;
use crate::mir::{Operand, Place, Rvalue, Terminator};
use crate::thir::{ThirCond, ThirExprId, ThirExprKind, ThirPatId, ThirPatKind, ThirStmt};
use crate::ty::{IntTy, Ty};

use super::rvalue::strip_refs;
use super::{BasicBlock, BlockAnd, Builder};

impl Builder<'_> {
    /// Evaluates `expr` and writes its value to `dest` (`None`: the value is discarded).
    pub(super) fn expr_into_dest(
        &mut self,
        dest: Option<Place>,
        block: BasicBlock,
        expr: ThirExprId,
    ) -> BlockAnd<()> {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            self.with_src(expr, |this| this.lower_into_dest(dest, block, expr))
        })
    }

    fn lower_into_dest(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        expr: ThirExprId,
    ) -> BlockAnd<()> {
        let thir = self.thir;
        let node = thir.expr(expr);
        match &node.kind {
            ThirExprKind::Block { stmts, tail } => {
                self.block_expr(dest, block, stmts, *tail, node.ty.is_never())
            }
            ThirExprKind::If { cond, then, else_ } => {
                self.if_expr(dest, block, *cond, *then, *else_)
            }
            ThirExprKind::Match { scrutinee, arms } => {
                self.match_expr(dest, block, *scrutinee, arms)
            }
            ThirExprKind::Logical { and, lhs, rhs } => self.logical(dest, block, *and, *lhs, *rhs),
            ThirExprKind::Loop { body } => self.loop_expr(dest, block, node.src, *body),
            ThirExprKind::While { cond, body } => {
                self.while_expr(dest, block, node.src, *cond, *body)
            }
            ThirExprKind::For {
                pat,
                iterable,
                body,
                kind,
            } => self.for_expr(dest, block, node.src, *pat, *iterable, *body, *kind),
            ThirExprKind::Break { target, value } => self.break_expr(block, *target, *value),
            ThirExprKind::Continue { target } => self.continue_expr(block, *target),
            ThirExprKind::Return { value } => self.return_expr(block, *value),
            ThirExprKind::Call { callee, args, kind } => {
                self.call(dest, block, expr, callee, args, *kind)
            }
            ThirExprKind::Assign { target, value } => {
                unpack!(block = self.assign_expr(block, *target, *value));
                self.assign_unit(block, dest);
                BlockAnd(block, ())
            }
            ThirExprKind::AssignOp {
                op,
                target,
                value,
                saturate,
            } => {
                unpack!(block = self.assign_op(block, *op, *target, *value, *saturate));
                self.assign_unit(block, dest);
                BlockAnd(block, ())
            }
            ThirExprKind::Try { expr: inner, kind } => self.try_expr(dest, block, *inner, *kind),
            ThirExprKind::Error => self.abort(block),
            // Coercions preserve the value at run time.
            ThirExprKind::Coerce { expr: inner, .. } => self.expr_into_dest(dest, block, *inner),
            _ => {
                let rvalue = unpack!(block = self.as_rvalue(block, expr));
                match dest {
                    Some(dest) => self.assign(block, dest, rvalue),
                    // A discarded constant needs no evaluation.
                    None if matches!(rvalue, Rvalue::Use(Operand::Const(_))) => {}
                    None => {
                        let temp = self.temp(node.ty.clone());
                        self.assign(block, Place::local(temp), rvalue);
                    }
                }
                BlockAnd(block, ())
            }
        }
    }

    /// A block; `diverges` when its type is `!`, so the end of its statements is never
    /// reached.
    fn block_expr(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        stmts: &[ThirStmt],
        tail: Option<ThirExprId>,
        diverges: bool,
    ) -> BlockAnd<()> {
        for stmt in stmts {
            match *stmt {
                ThirStmt::Let {
                    pat,
                    init: Some(init),
                } => unpack!(block = self.let_binding(block, pat, init)),
                // The bindings of `let pat;` are assigned later.
                ThirStmt::Let { init: None, .. } => {}
                ThirStmt::Expr(expr) => unpack!(block = self.expr_into_dest(None, block, expr)),
            }
        }
        match tail {
            Some(tail) => self.expr_into_dest(dest, block, tail),
            None if diverges => self.abort(block),
            None => {
                self.assign_unit(block, dest);
                BlockAnd(block, ())
            }
        }
    }

    /// `let pat = init;`: a plain binding is written directly; any other pattern
    /// destructures the place of `init`.
    pub(super) fn let_binding(
        &mut self,
        mut block: BasicBlock,
        pat: ThirPatId,
        init: ThirExprId,
    ) -> BlockAnd<()> {
        if let ThirPatKind::Binding {
            local,
            by_ref: None,
        } = self.thir.pat(pat).kind
        {
            let local = self.local(local);
            return self.expr_into_dest(Some(Place::local(local)), block, init);
        }
        let place = unpack!(block = self.as_place(block, init));
        block = self.with_src(init, |this| this.bind_irrefutable(block, place, pat));
        BlockAnd(block, ())
    }

    fn if_expr(
        &mut self,
        dest: Option<Place>,
        block: BasicBlock,
        cond: ThirCond,
        then: ThirExprId,
        else_: Option<ThirExprId>,
    ) -> BlockAnd<()> {
        let then_block = self.new_block();
        let else_block = self.new_block();
        self.cond(block, cond, then_block, else_block);
        let BlockAnd(then_end, ()) = self.expr_into_dest(dest.clone(), then_block, then);
        let else_end = match else_ {
            Some(else_) => {
                let BlockAnd(end, ()) = self.expr_into_dest(dest, else_block, else_);
                end
            }
            None => {
                self.assign_unit(else_block, dest);
                else_block
            }
        };
        let join = self.new_block();
        self.goto(then_end, join);
        self.goto(else_end, join);
        BlockAnd(join, ())
    }

    /// Branches on `cond` from `block` to `then` (with a `let`'s bindings bound) or
    /// `other`.
    pub(super) fn cond(
        &mut self,
        mut block: BasicBlock,
        cond: ThirCond,
        then: BasicBlock,
        other: BasicBlock,
    ) {
        match cond {
            ThirCond::Expr(expr) => {
                let cond = unpack!(block = self.as_operand(block, expr));
                self.with_src(expr, |this| {
                    this.terminate(
                        block,
                        Terminator::SwitchBool {
                            cond,
                            then,
                            other,
                            short_circuit: None,
                        },
                    );
                });
            }
            ThirCond::Let { pat, expr } => {
                let place = unpack!(block = self.as_place(block, expr));
                self.with_src(expr, |this| this.test_let(block, place, pat, then, other));
            }
        }
    }

    /// `lhs && rhs` or `lhs || rhs`: two branches deciding the result local.
    fn logical(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        and: bool,
        lhs: ThirExprId,
        rhs: ThirExprId,
    ) -> BlockAnd<()> {
        let result = match dest.as_ref().and_then(Place::as_local) {
            Some(local) => local,
            None => self.temp_mut(Ty::bool()),
        };
        let left = unpack!(block = self.as_operand(block, lhs));
        let rhs_block = self.new_block();
        let true_block = self.new_block();
        let false_block = self.new_block();
        let (then, other) = if and {
            (rhs_block, false_block)
        } else {
            (true_block, rhs_block)
        };
        self.terminate(
            block,
            Terminator::SwitchBool {
                cond: left,
                then,
                other,
                short_circuit: Some(result),
            },
        );
        let mut rhs_end = rhs_block;
        let right = unpack!(rhs_end = self.as_operand(rhs_end, rhs));
        self.terminate(
            rhs_end,
            Terminator::SwitchBool {
                cond: right,
                then: true_block,
                other: false_block,
                short_circuit: Some(result),
            },
        );
        let join = self.new_block();
        for (target, value) in [(true_block, true), (false_block, false)] {
            self.assign(
                target,
                Place::local(result),
                Rvalue::Use(Self::const_operand(ConstValue::Bool(value))),
            );
            self.goto(target, join);
        }
        if let Some(dest) = dest
            && dest.as_local() != Some(result)
        {
            self.assign(join, dest, Rvalue::Use(Operand::Copy(Place::local(result))));
        }
        BlockAnd(join, ())
    }

    /// `target = value`: the value first, then the place.
    fn assign_expr(
        &mut self,
        mut block: BasicBlock,
        target: ThirExprId,
        value: ThirExprId,
    ) -> BlockAnd<()> {
        let rvalue = if self.is_pure(target) {
            unpack!(block = self.as_rvalue(block, value))
        } else {
            Rvalue::Use(unpack!(block = self.as_stable_operand(block, value)))
        };
        let place = unpack!(block = self.as_place(block, target));
        self.assign(block, place, rvalue);
        BlockAnd(block, ())
    }

    /// `target op= value`: the value, then the place, read, computed and written back. An
    /// `Int<lo, hi>` computes in `i128` and saturates at its bounds.
    fn assign_op(
        &mut self,
        mut block: BasicBlock,
        op: BinaryOp,
        target: ThirExprId,
        value: ThirExprId,
        saturate: bool,
    ) -> BlockAnd<()> {
        let value = if self.is_pure(target) {
            unpack!(block = self.as_operand(block, value))
        } else {
            unpack!(block = self.as_stable_operand(block, value))
        };
        let place = unpack!(block = self.as_place(block, target));
        let target_ty = strip_refs(&self.thir.expr(target).ty);
        let current = Operand::Copy(place.clone());
        if saturate {
            let wide = Ty::int(IntTy::I128);
            let temp = self.temp(wide.clone());
            self.assign(
                block,
                Place::local(temp),
                Rvalue::IntBin {
                    op,
                    lhs: current,
                    rhs: value,
                    ty: wide,
                },
            );
            self.assign(
                block,
                place,
                Rvalue::IntClamp {
                    operand: Operand::Copy(Place::local(temp)),
                    ty: target_ty,
                },
            );
        } else {
            let rvalue = self.binary_rvalue(op, current, value, &target_ty, &target_ty);
            self.assign(block, place, rvalue);
        }
        BlockAnd(block, ())
    }
}

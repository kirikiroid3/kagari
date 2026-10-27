//! Loops, `break`/`continue`/`return` and `?`.
//!
//! `for` is desugared into a counter loop: over a range, `i = start` counts to `end`
//! (`i` is a hidden local of the loop variable's type, incremented in `i128`); over an
//! array or `Vec`, an index counts to its length. `continue` goes to the increment.

use la_arena::RawIdx;
use smallvec::smallvec;

use crate::eval::ConstValue;
use crate::hir::{BinaryOp, ExprId};
use crate::mir::{AggregateKind, Body, Local, LoopInfo, Operand, Place, Proj, Rvalue, Terminator};
use crate::thir::{ForKind, ThirCond, ThirExprId, ThirPatId, TryKind};
use crate::ty::{CtorRef, IntTy, Ty, TyKind};

use super::{BasicBlock, BlockAnd, Builder, LoopScope};

impl Builder<'_> {
    /// Runs `f` (which lowers a loop's condition and body) inside the scope of the loop
    /// `target`, and records the loop's region when `record`.
    fn in_loop(
        &mut self,
        scope: LoopScope,
        header: BasicBlock,
        record: bool,
        f: impl FnOnce(&mut Self),
    ) {
        let target = scope.target;
        let first = self.blocks.len();
        self.scopes.push(scope);
        f(self);
        self.scopes.pop();
        if record {
            let last = self.blocks.len();
            let mut region = vec![header];
            region.extend((first..last).filter_map(|index| {
                u32::try_from(index)
                    .ok()
                    .map(|index| BasicBlock::from_raw(RawIdx::from_u32(index)))
            }));
            self.loops.push(LoopInfo {
                src: target,
                header,
                region,
            });
        }
    }

    pub(super) fn loop_expr(
        &mut self,
        dest: Option<Place>,
        block: BasicBlock,
        src: ExprId,
        body: ThirExprId,
    ) -> BlockAnd<()> {
        let exit = self.new_block();
        let header = self.new_block();
        self.goto(block, header);
        let scope = LoopScope {
            target: src,
            brk: exit,
            cont: header,
            dest,
        };
        self.in_loop(scope, header, true, |this| {
            let BlockAnd(end, ()) = this.expr_into_dest(None, header, body);
            this.goto(end, header);
        });
        BlockAnd(exit, ())
    }

    pub(super) fn while_expr(
        &mut self,
        dest: Option<Place>,
        block: BasicBlock,
        src: ExprId,
        cond: ThirCond,
        body: ThirExprId,
    ) -> BlockAnd<()> {
        let exit = self.new_block();
        let header = self.new_block();
        self.goto(block, header);
        let scope = LoopScope {
            target: src,
            brk: exit,
            cont: header,
            dest: None,
        };
        self.in_loop(scope, header, true, |this| {
            let body_block = this.new_block();
            this.cond(header, cond, body_block, exit);
            let BlockAnd(end, ()) = this.expr_into_dest(None, body_block, body);
            this.goto(end, header);
        });
        self.assign_unit(exit, dest);
        BlockAnd(exit, ())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn for_expr(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        src: ExprId,
        pat: ThirPatId,
        iterable: ThirExprId,
        body: ThirExprId,
        kind: ForKind,
    ) -> BlockAnd<()> {
        let iter = unpack!(block = self.as_temp(block, iterable));
        let iter = Place::local(iter);
        let exit = self.new_block();
        let incr = self.new_block();
        // The block the body starts in (the pattern bound) and the place it binds from.
        let (body_block, element) = match kind {
            ForKind::Range { inclusive } => {
                let field = |name: &str| {
                    self.cx
                        .std
                        .range
                        .field(name)
                        .and_then(|index| u32::try_from(index).ok())
                };
                let (Some(start), Some(end), Some(flag)) =
                    (field("start"), field("end"), field("inclusive"))
                else {
                    return self.abort(block);
                };
                let start = Operand::Copy(iter.clone().project(Proj::Field(start)));
                let end = Operand::Copy(iter.clone().project(Proj::Field(end)));
                let flag = Operand::Copy(iter.clone().project(Proj::Field(flag)));
                let counter = self.temp_mut(self.thir.pat(pat).ty.clone());
                let body_block =
                    self.range_loop(block, counter, start, end, flag, inclusive, exit, incr);
                (body_block, Place::local(counter))
            }
            ForKind::Array | ForKind::Vec => {
                let index_ty = Ty::int(IntTy::Usize);
                let len = self.temp(index_ty.clone());
                self.assign(block, Place::local(len), Rvalue::Len(iter.clone()));
                let index = self.temp_mut(index_ty);
                self.assign(
                    block,
                    Place::local(index),
                    Rvalue::Use(Self::const_operand(ConstValue::Int(0))),
                );
                let header = self.new_block();
                self.goto(block, header);
                let body_block = self.new_block();
                let more = self.compare(
                    header,
                    BinaryOp::Lt,
                    Self::copy(index),
                    Operand::Copy(Place::local(len)),
                );
                self.switch(header, more, body_block, exit);
                self.increment(incr, index, header);
                (body_block, iter.project(Proj::Index(index)))
            }
        };
        let scope = LoopScope {
            target: src,
            brk: exit,
            cont: incr,
            dest: None,
        };
        self.in_loop(scope, body_block, false, |this| {
            let block = this.bind_irrefutable(body_block, element, pat);
            let BlockAnd(end, ()) = this.expr_into_dest(None, block, body);
            this.goto(end, incr);
        });
        self.assign_unit(exit, dest);
        BlockAnd(exit, ())
    }

    /// The counter loop over a range from `block`; `incr` (allocated) gets the increment.
    /// Returns the block the body starts in, with `counter` holding the element.
    #[allow(clippy::too_many_arguments)]
    fn range_loop(
        &mut self,
        block: BasicBlock,
        counter: Local,
        start: Operand,
        end: Operand,
        flag: Operand,
        inclusive: Option<bool>,
        exit: BasicBlock,
        incr: BasicBlock,
    ) -> BasicBlock {
        let body_block = self.new_block();
        match inclusive {
            // `i = start; while i < end { ..; i += 1 }`
            Some(false) => {
                self.assign(block, Place::local(counter), Rvalue::Use(start));
                let header = self.new_block();
                self.goto(block, header);
                let more = self.compare(header, BinaryOp::Lt, Self::copy(counter), end);
                self.switch(header, more, body_block, exit);
                self.increment(incr, counter, header);
            }
            // `if start <= end { i = start; loop { ..; if i == end break; i += 1 } }`
            Some(true) => {
                let empty = self.compare(block, BinaryOp::Gt, start.clone(), end.clone());
                let init = self.new_block();
                self.switch(block, empty, exit, init);
                self.assign(init, Place::local(counter), Rvalue::Use(start));
                self.goto(init, body_block);
                self.last_or_increment(incr, counter, end, exit, body_block);
            }
            // The range's `inclusive` field decides at run time:
            // `i = start; while i < end || inclusive && i == end { ..; if i == end break; i += 1 }`
            None => {
                self.assign(block, Place::local(counter), Rvalue::Use(start));
                let header = self.new_block();
                self.goto(block, header);
                let below = self.compare(header, BinaryOp::Lt, Self::copy(counter), end.clone());
                let at_end = self.new_block();
                self.switch(header, below, body_block, at_end);
                let check = self.new_block();
                self.switch(at_end, flag, check, exit);
                let equal = self.compare(check, BinaryOp::Eq, Self::copy(counter), end.clone());
                self.switch(check, equal, body_block, exit);
                self.last_or_increment(incr, counter, end, exit, header);
            }
        }
        body_block
    }

    /// In `incr`: `if counter == end { goto exit } counter += 1; goto next`.
    fn last_or_increment(
        &mut self,
        incr: BasicBlock,
        counter: Local,
        end: Operand,
        exit: BasicBlock,
        next: BasicBlock,
    ) {
        let last = self.compare(incr, BinaryOp::Eq, Self::copy(counter), end);
        let step = self.new_block();
        self.switch(incr, last, exit, step);
        self.increment(step, counter, next);
    }

    /// In `block`: `counter += 1` (in `i128`), then `goto next`.
    fn increment(&mut self, block: BasicBlock, counter: Local, next: BasicBlock) {
        self.assign(
            block,
            Place::local(counter),
            Rvalue::IntBin {
                op: BinaryOp::Add,
                lhs: Self::copy(counter),
                rhs: Self::const_operand(ConstValue::Int(1)),
                ty: Ty::int(IntTy::I128),
            },
        );
        self.goto(block, next);
    }

    fn copy(local: Local) -> Operand {
        Operand::Copy(Place::local(local))
    }

    /// A `bool` temporary assigned `lhs op rhs` in `block`.
    pub(super) fn compare(
        &mut self,
        block: BasicBlock,
        op: BinaryOp,
        lhs: Operand,
        rhs: Operand,
    ) -> Operand {
        let temp = self.temp(Ty::bool());
        self.assign(block, Place::local(temp), Rvalue::Cmp { op, lhs, rhs });
        Self::copy(temp)
    }

    /// Ends `block` with a branch on `cond`.
    pub(super) fn switch(
        &mut self,
        block: BasicBlock,
        cond: Operand,
        then: BasicBlock,
        other: BasicBlock,
    ) {
        self.terminate(
            block,
            Terminator::SwitchBool {
                cond,
                then,
                other,
                short_circuit: None,
            },
        );
    }

    pub(super) fn break_expr(
        &mut self,
        mut block: BasicBlock,
        target: ExprId,
        value: Option<ThirExprId>,
    ) -> BlockAnd<()> {
        let Some(scope) = self
            .scopes
            .iter()
            .rev()
            .find(|scope| scope.target == target)
        else {
            return self.abort(block);
        };
        let (brk, dest) = (scope.brk, scope.dest.clone());
        match value {
            Some(value) => unpack!(block = self.expr_into_dest(dest, block, value)),
            None => self.assign_unit(block, dest),
        }
        self.goto(block, brk);
        BlockAnd(self.new_block(), ())
    }

    pub(super) fn continue_expr(&mut self, block: BasicBlock, target: ExprId) -> BlockAnd<()> {
        let Some(scope) = self
            .scopes
            .iter()
            .rev()
            .find(|scope| scope.target == target)
        else {
            return self.abort(block);
        };
        let cont = scope.cont;
        self.goto(block, cont);
        BlockAnd(self.new_block(), ())
    }

    pub(super) fn return_expr(
        &mut self,
        mut block: BasicBlock,
        value: Option<ThirExprId>,
    ) -> BlockAnd<()> {
        let dest = Some(Place::local(Body::RETURN));
        match value {
            Some(value) => unpack!(block = self.expr_into_dest(dest, block, value)),
            None => self.assign_unit(block, dest),
        }
        self.terminate(block, Terminator::Return);
        BlockAnd(self.new_block(), ())
    }

    /// `inner?`: the success variant's field, or a return of the failure (`None`, or
    /// `Err` of the error) from the body.
    pub(super) fn try_expr(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        inner: ThirExprId,
        kind: TryKind,
    ) -> BlockAnd<()> {
        let TyKind::Adt { item: adt, .. } = self.thir.expr(inner).ty.kind() else {
            return self.abort(block);
        };
        let adt = *adt;
        let place = unpack!(block = self.as_place(block, inner));
        let (success, failure, carries_error) = match kind {
            TryKind::Option { some, none } => (some, none, false),
            TryKind::Result { ok, err } => (ok, err, true),
        };
        let ok_block = self.new_block();
        let fail_block = self.new_block();
        self.terminate(
            block,
            Terminator::SwitchVariant {
                place: place.clone(),
                adt,
                targets: smallvec![(success, ok_block), (failure, fail_block)],
                otherwise: None,
            },
        );
        // The failure, rebuilt in the body's return type.
        let ret_args = match self.locals[Body::RETURN].ty.kind() {
            TyKind::Adt { item, args } if *item == adt => Some(args.clone()),
            _ => None,
        };
        match ret_args {
            Some(args) => {
                let ops = if carries_error {
                    smallvec![Operand::Copy(
                        place
                            .clone()
                            .project(Proj::Downcast(failure))
                            .project(Proj::Field(0))
                    )]
                } else {
                    smallvec![]
                };
                self.assign(
                    fail_block,
                    Place::local(Body::RETURN),
                    Rvalue::Aggregate {
                        kind: AggregateKind::Adt(CtorRef {
                            adt,
                            variant: Some(failure),
                            args,
                        }),
                        ops,
                    },
                );
                self.terminate(fail_block, Terminator::Return);
            }
            None => self.terminate(fail_block, Terminator::Unreachable),
        }
        let value = Operand::Copy(
            place
                .project(Proj::Downcast(success))
                .project(Proj::Field(0)),
        );
        if let Some(dest) = dest {
            self.assign(ok_block, dest, Rvalue::Use(value));
        }
        BlockAnd(ok_block, ())
    }
}

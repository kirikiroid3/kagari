//! Calls: `push_step`, `rng_choose` and scene calls end their block with a suspension
//! terminator; any other call is a statement. The callee value (if any) is evaluated
//! first, then the arguments left to right (a method's receiver first).

use crate::callee::CalleeKind;
use crate::mir::{Callee, Place, StatementKind, Terminator};
use crate::thir::{ThirCallee, ThirExprId};
use crate::ty::GenericArg;

use super::{BasicBlock, BlockAnd, Builder};

impl Builder<'_> {
    pub(super) fn call(
        &mut self,
        dest: Option<Place>,
        mut block: BasicBlock,
        expr: ThirExprId,
        callee: &ThirCallee,
        args: &[ThirExprId],
        kind: CalleeKind,
    ) -> BlockAnd<()> {
        let callee = match callee {
            ThirCallee::Direct(fn_ref) => Callee::Fn(fn_ref.clone()),
            ThirCallee::Builtin(builtin, fn_ref) => Callee::Builtin(*builtin, fn_ref.clone()),
            ThirCallee::Value(value) => {
                let effects = args.iter().any(|arg| !self.is_pure(*arg));
                let value = if effects {
                    unpack!(block = self.as_stable_operand(block, *value))
                } else {
                    unpack!(block = self.as_operand(block, *value))
                };
                Callee::Value(value)
            }
        };
        let args = unpack!(block = self.as_operands(block, args));
        let dest = match dest {
            Some(dest) => dest,
            None => Place::local(self.temp(self.thir.expr(expr).ty.clone())),
        };
        match kind {
            CalleeKind::Step => {
                let Some(arg) = args.into_iter().next() else {
                    return self.abort(block);
                };
                let next = self.new_block();
                self.terminate(block, Terminator::Step { arg, dest, next });
                BlockAnd(next, ())
            }
            CalleeKind::Rng => {
                let fn_ref = match &callee {
                    Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => Some(fn_ref),
                    Callee::Value(_) => None,
                };
                let Some(GenericArg::Type(ty)) = fn_ref.and_then(|fn_ref| fn_ref.args.first())
                else {
                    return self.abort(block);
                };
                let ty = ty.clone();
                let next = self.new_block();
                self.terminate(block, Terminator::Rng { ty, dest, next });
                BlockAnd(next, ())
            }
            CalleeKind::Scene => {
                let next = self.new_block();
                self.terminate(
                    block,
                    Terminator::SceneCall {
                        callee,
                        args,
                        dest,
                        next,
                    },
                );
                BlockAnd(next, ())
            }
            CalleeKind::Plain | CalleeKind::Const | CalleeKind::Instant => {
                self.push(block, StatementKind::Call { dest, callee, args });
                BlockAnd(block, ())
            }
        }
    }
}

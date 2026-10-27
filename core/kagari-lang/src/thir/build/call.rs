//! Calls and method calls.

use crate::callee::{Callee, CalleeKind, classify};
use crate::hir::{Expr, ExprId};
use crate::infer::ValueRef;
use crate::thir::{ThirCallee, ThirExprKind};
use crate::ty::{FnDefId, FnRef, Ty, TyKind};

use super::Builder;

impl Builder<'_> {
    pub(super) fn call(&mut self, callee: ExprId, args: &[ExprId]) -> ThirExprKind {
        // A path to a function is called directly; anything else is evaluated to a value.
        let direct = match (self.body.store.expr(callee), self.infer.paths.get(&callee)) {
            (Expr::Path(_), Some(ValueRef::Fn(fn_ref))) => Some(fn_ref.clone()),
            _ => None,
        };
        let (callee, kind) = match direct {
            Some(fn_ref) => {
                let kind = self.fn_kind(&fn_ref);
                (self.direct(fn_ref), kind)
            }
            None => {
                let value = self.expr(callee);
                let kind = self.value_kind(&self.ty_of(value));
                (ThirCallee::Value(value), kind)
            }
        };
        let Some(kind) = kind else {
            return ThirExprKind::Error;
        };
        let args = args.iter().map(|arg| self.expr(*arg)).collect();
        ThirExprKind::Call { callee, args, kind }
    }

    /// A method call: the receiver, dereferenced and borrowed as inference adjusted it, is
    /// the first argument.
    pub(super) fn method_call(
        &mut self,
        expr: ExprId,
        receiver: ExprId,
        args: &[ExprId],
    ) -> ThirExprKind {
        let Some(call) = self.infer.method_calls.get(&expr) else {
            return ThirExprKind::Error;
        };
        let Some(kind) = self.fn_kind(&call.func) else {
            return ThirExprKind::Error;
        };
        let receiver = self.expr(receiver);
        let mut receiver = self.deref_n(receiver, call.receiver.derefs, expr);
        if let Some(mutable) = call.receiver.autoref {
            let ty = Ty::reference(mutable, self.ty_of(receiver));
            receiver = self.alloc(
                ThirExprKind::Borrow {
                    mutable,
                    place: receiver,
                },
                ty,
                expr,
            );
        }
        let mut values = Vec::with_capacity(args.len().saturating_add(1));
        values.push(receiver);
        values.extend(args.iter().map(|arg| self.expr(*arg)));
        ThirExprKind::Call {
            callee: self.direct(call.func.clone()),
            args: values,
            kind,
        }
    }

    /// A named callee: a builtin or an ordinary function (or constructor).
    fn direct(&self, fn_ref: FnRef) -> ThirCallee {
        match fn_ref.def {
            FnDefId::Item(item) => match self.std.builtin(item) {
                Some(builtin) => ThirCallee::Builtin(builtin, fn_ref),
                None => ThirCallee::Direct(fn_ref),
            },
            FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => ThirCallee::Direct(fn_ref),
        }
    }

    /// The kind of a named callee; constructors are constant.
    fn fn_kind(&self, fn_ref: &FnRef) -> Option<CalleeKind> {
        match fn_ref.def {
            FnDefId::Item(item) => classify(self.sigs, &Callee::Item(item)),
            FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => Some(CalleeKind::Const),
        }
    }

    /// The kind of a callee value of type `ty`.
    fn value_kind(&self, ty: &Ty) -> Option<CalleeKind> {
        match ty.kind() {
            TyKind::FnDef(fn_ref) => self.fn_kind(fn_ref),
            _ => classify(self.sigs, &Callee::Value(ty.clone())),
        }
    }
}

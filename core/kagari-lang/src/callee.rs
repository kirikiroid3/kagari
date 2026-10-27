//! What kind of function a call calls, shared by the kind checks and THIR building.

use crate::hir::{ClosureKind, FnPtrKind};
use crate::item_tree::{FnKind, ItemId};
use crate::signatures::ProgramSignatures;
use crate::ty::{Ty, TyKind};

/// What a call calls, by kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalleeKind {
    Plain,
    Const,
    Instant,
    Scene,
    /// `push_step`.
    Step,
    /// `rng_choose`.
    Rng,
}

/// A called function: a named one or a value of pointer or closure type.
#[derive(Clone, Debug)]
pub(crate) enum Callee {
    Item(ItemId),
    Value(Ty),
}

/// The kind of `callee`; `None` for a value that is not a pointer or closure, or an item
/// without a function signature.
pub(crate) fn classify(sigs: &ProgramSignatures, callee: &Callee) -> Option<CalleeKind> {
    match callee {
        Callee::Item(item) => {
            let sig = sigs.fn_sig(*item)?;
            Some(match sig.kind {
                FnKind::Plain => CalleeKind::Plain,
                FnKind::Const => CalleeKind::Const,
                FnKind::Pushdown if Some(*item) == sigs.std.push_step => CalleeKind::Step,
                FnKind::Pushdown if Some(*item) == sigs.std.rng_choose => CalleeKind::Rng,
                FnKind::Pushdown if sig.scene => CalleeKind::Scene,
                FnKind::Pushdown => CalleeKind::Instant,
            })
        }
        Callee::Value(ty) => match ty.kind() {
            TyKind::FnPtr { kind, .. } => Some(match kind {
                FnPtrKind::Plain => CalleeKind::Plain,
                FnPtrKind::Const => CalleeKind::Const,
                FnPtrKind::Pushdown => CalleeKind::Instant,
                FnPtrKind::Scene => CalleeKind::Scene,
            }),
            TyKind::Closure { kind, .. } => Some(match kind {
                ClosureKind::Plain => CalleeKind::Plain,
                ClosureKind::Const => CalleeKind::Const,
                ClosureKind::Pushdown => CalleeKind::Instant,
            }),
            _ => None,
        },
    }
}

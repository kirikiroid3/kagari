//! The narrative IR: one control-flow graph per scene over finite variable domains.
//!
//! This is the attachment point for stage 4 (model checking), which will consume
//! [`narrative_ir`](lower::narrative_ir). Stage 2's termination check runs on it now.

pub mod lower;
pub mod termination;

use la_arena::Idx;
use smol_str::SmolStr;

use crate::diagnostic::Span;
use crate::kis::hir::{Render, SceneDecl, VarDecl};

pub use lower::narrative_ir;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NarrativeIr {
    pub vars: Vec<IrVar>,
    /// Same order as `GameHir::scenes`.
    pub scenes: Vec<IrScene>,
    pub entries: Vec<IrEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SceneId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrVar {
    pub decl: Idx<VarDecl>,
    pub domain: Domain,
    /// Domain index of the initial value.
    pub init: u32,
    pub local_to: Option<SceneId>,
    pub from_meta: bool,
}

/// A finite variable domain. Values are domain indices: bool false = 0, true = 1;
/// enum = variant index; int/char = `value - min` (char min = 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    Bool,
    Enum { size: u32 },
    Range { min: i32, max: i32 },
}

impl Domain {
    pub fn size(&self) -> u32 {
        match *self {
            Domain::Bool => 2,
            Domain::Enum { size } => size,
            Domain::Range { min, max } => {
                let span = i64::from(max)
                    .saturating_sub(i64::from(min))
                    .saturating_add(1);
                u32::try_from(span.max(0)).unwrap_or(u32::MAX)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrScene {
    pub decl: Idx<SceneDecl>,
    pub nodes: Vec<Node>,
    pub entry: NodeId,
    pub exit: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub op: Op,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Say {
        renders: Vec<Idx<Render>>,
        next: NodeId,
    },
    Choice {
        id: SmolStr,
        renders: Vec<Idx<Render>>,
        matches: Vec<IrChoiceMatch>,
        arms: Vec<IrArm>,
    },
    Assign {
        var: VarId,
        value: u32,
        next: NodeId,
    },
    /// Saturating.
    Add {
        var: VarId,
        amount: u32,
        next: NodeId,
    },
    /// Saturating.
    Sub {
        var: VarId,
        amount: u32,
        next: NodeId,
    },
    /// First matching arm wins; arms are exhaustive.
    Branch {
        vars: Vec<VarId>,
        arms: Vec<IrBranchArm>,
    },
    Call {
        scene: SceneId,
        next: NodeId,
    },
    /// Writes a meta value; no effect on the game state.
    MetaWrite {
        next: NodeId,
    },
    /// Label, goto, join, return (to exit), unresolved goto/call.
    Jump {
        next: NodeId,
        from_goto: bool,
    },
    Exit,
}

/// Patterns over domain indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrPattern {
    Any,
    Range { lo: u32, hi: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrBranchArm {
    pub rows: Vec<Vec<IrPattern>>,
    pub target: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrChoiceMatch {
    pub vars: Vec<VarId>,
    /// Per arm, its rows.
    pub arms: Vec<Vec<Vec<IrPattern>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrArm {
    pub id: SmolStr,
    /// `(match index, arm index)`: offered iff every listed match selects that arm.
    pub guard: Vec<(u32, u32)>,
    pub target: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrEntry {
    pub name: SmolStr,
    pub scene: SceneId,
    pub exit_var: Option<VarId>,
}

//! CFG construction from [`GameHir`](crate::kis::hir::GameHir).
//!
//! Each block is lowered back-to-front with a `next` continuation, so control-flow
//! joins need no extra nodes: every branch of a `<match>` or `<choice>` targets the
//! continuation directly.

use la_arena::Idx;

use crate::db::Db;
use crate::diagnostic::Span;
use crate::input::Program;
use crate::ir::{
    Domain, IrArm, IrBranchArm, IrChoiceMatch, IrPattern, IrReturn, IrScene, IrVar, NarrativeIr,
    Node, NodeId, Op, SceneId, VarId,
};
use crate::kis::game_hir;
use crate::kis::hir::{
    Block, ChoiceItem, GameHir, MatchStmt, ReturnValue, SceneDecl, StmtKind, VarDecl, VarScope,
};
use crate::kis::value::{Pattern, Value, VarType};

#[salsa::tracked(returns(ref))]
pub fn narrative_ir(db: &dyn Db, program: Program) -> NarrativeIr {
    let hir = game_hir(db, program);
    let vars: Vec<IrVar> = hir
        .vars
        .iter()
        .map(|(idx, decl)| lower_var(hir, idx, decl))
        .collect();
    let scenes = hir
        .scenes
        .iter()
        .map(|(idx, scene)| {
            let returns = scene.returns.map(|ty| domain(hir, ty));
            SceneBuilder::new(&vars, idx, scene, returns).finish(scene)
        })
        .collect();
    let entries = hir
        .scenes
        .iter()
        .filter(|(_, scene)| scene.entry)
        .map(|(idx, _)| scene_id(idx))
        .collect();
    NarrativeIr {
        vars,
        scenes,
        entries,
    }
}

fn raw(idx: Idx<impl Sized>) -> u32 {
    u32::from(idx.into_raw())
}

fn var_id(idx: Idx<VarDecl>) -> VarId {
    VarId(raw(idx))
}

fn scene_id(idx: Idx<SceneDecl>) -> SceneId {
    SceneId(raw(idx))
}

fn domain(hir: &GameHir, ty: VarType) -> Domain {
    match ty {
        VarType::Bool => Domain::Bool,
        VarType::Enum(e) => Domain::Enum {
            size: u32::try_from(hir.enums[e].variants.len()).unwrap_or(u32::MAX),
        },
        VarType::Char => Domain::Range { min: 0, max: 255 },
        VarType::Int { min, max } => Domain::Range {
            min: i32::from(min),
            max: i32::from(max),
        },
    }
}

fn lower_var(hir: &GameHir, idx: Idx<VarDecl>, decl: &VarDecl) -> IrVar {
    let domain = domain(hir, decl.ty);
    IrVar {
        decl: idx,
        domain,
        init: value_index(domain, decl.init),
        local_to: match decl.scope {
            VarScope::Global => None,
            VarScope::Local(scene) => Some(scene_id(scene)),
        },
        from_meta: decl.from_meta.is_some(),
    }
}

/// The domain index of `value`. Stage 2 only produces values of the variable's type;
/// anything else maps to 0.
fn value_index(domain: Domain, value: Value) -> u32 {
    match (domain, value) {
        (Domain::Bool, Value::Bool(b)) => u32::from(b),
        (Domain::Enum { .. }, Value::Variant(index)) => index,
        (Domain::Range { min, .. }, Value::Int(v)) => int_index(min, v),
        _ => 0,
    }
}

fn int_index(min: i32, v: i32) -> u32 {
    u32::try_from(i64::from(v).saturating_sub(i64::from(min))).unwrap_or(0)
}

fn lower_pattern(domain: Domain, pattern: Pattern) -> IrPattern {
    match pattern {
        Pattern::Wildcard => IrPattern::Any,
        Pattern::Value(value) => {
            let index = value_index(domain, value);
            IrPattern::Range {
                lo: index,
                hi: index,
            }
        }
        Pattern::Range(lo, hi) => {
            let min = match domain {
                Domain::Range { min, .. } => min,
                Domain::Bool | Domain::Enum { .. } => 0,
            };
            IrPattern::Range {
                lo: int_index(min, lo),
                hi: int_index(min, hi),
            }
        }
    }
}

struct SceneBuilder<'a> {
    vars: &'a [IrVar],
    decl: Idx<SceneDecl>,
    returns: Option<Domain>,
    nodes: Vec<Node>,
    exit: NodeId,
    /// One pre-allocated `Jump` node per `SceneDecl::labels` entry, so gotos can target
    /// labels that are lowered later.
    labels: Vec<NodeId>,
    /// Whether the label's node has been placed by its `Label` statement.
    placed: Vec<bool>,
}

impl<'a> SceneBuilder<'a> {
    fn new(
        vars: &'a [IrVar],
        decl: Idx<SceneDecl>,
        scene: &SceneDecl,
        returns: Option<Domain>,
    ) -> SceneBuilder<'a> {
        let mut builder = SceneBuilder {
            vars,
            decl,
            returns,
            nodes: Vec::new(),
            exit: NodeId(0),
            labels: Vec::new(),
            placed: vec![false; scene.labels.len()],
        };
        builder.exit = builder.push(Op::Exit, scene.span);
        let exit = builder.exit;
        builder.labels = scene
            .labels
            .iter()
            .map(|label| {
                builder.push(
                    Op::Jump {
                        next: exit,
                        from_goto: false,
                    },
                    label.span,
                )
            })
            .collect();
        builder
    }

    fn finish(mut self, scene: &SceneDecl) -> IrScene {
        let entry = self.block(&scene.body, self.exit);
        IrScene {
            decl: self.decl,
            returns: self.returns,
            nodes: self.nodes,
            entry,
            exit: self.exit,
        }
    }

    fn push(&mut self, op: Op, span: Span) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(Node { op, span });
        id
    }

    fn var(&self, idx: Idx<VarDecl>) -> (VarId, Domain) {
        let id = var_id(idx);
        let domain = usize::try_from(id.0)
            .ok()
            .and_then(|i| self.vars.get(i))
            .map_or(Domain::Bool, |var| var.domain);
        (id, domain)
    }

    fn block(&mut self, block: &Block, next: NodeId) -> NodeId {
        block
            .stmts
            .iter()
            .rev()
            .fold(next, |next, stmt| self.stmt(&stmt.kind, stmt.span, next))
    }

    fn stmt(&mut self, kind: &StmtKind, span: Span, next: NodeId) -> NodeId {
        let op = match kind {
            StmtKind::Say { renders, .. } => Op::Say {
                renders: renders.clone(),
                next,
            },
            StmtKind::Choice { id, renders, items } => {
                let mut matches = Vec::new();
                let mut arms = Vec::new();
                self.choice_items(items, &mut Vec::new(), &mut matches, &mut arms, next);
                Op::Choice {
                    id: id.clone(),
                    renders: renders.clone(),
                    matches,
                    arms,
                }
            }
            StmtKind::Label(index) => return self.label(*index, span, next),
            StmtKind::Goto(Some(index)) => {
                let target = usize::try_from(*index)
                    .ok()
                    .and_then(|i| self.labels.get(i).copied());
                match target {
                    Some(target) => Op::Jump {
                        next: target,
                        from_goto: true,
                    },
                    None => Op::Jump {
                        next,
                        from_goto: false,
                    },
                }
            }
            StmtKind::Goto(None) | StmtKind::Call { scene: None, .. } => Op::Jump {
                next,
                from_goto: false,
            },
            StmtKind::Call {
                scene: Some(scene),
                assign,
            } => Op::Call {
                scene: scene_id(*scene),
                assign: assign.map(var_id),
                next,
            },
            StmtKind::Return(value) => Op::Return {
                value: value.map(|value| match value {
                    ReturnValue::Var(var) => IrReturn::Var(var_id(var)),
                    ReturnValue::Literal(literal) => IrReturn::Literal(
                        self.returns
                            .map_or(0, |domain| value_index(domain, literal)),
                    ),
                }),
                next: self.exit,
            },
            StmtKind::Match(m) => {
                let (vars, domains) = self.match_vars(m);
                let arms = m
                    .arms
                    .iter()
                    .map(|arm| IrBranchArm {
                        rows: lower_rows(&domains, &arm.rows),
                        target: self.block(&arm.body, next),
                    })
                    .collect();
                Op::Branch { vars, arms }
            }
            StmtKind::Set { var, value } => {
                let (var, domain) = self.var(*var);
                Op::Assign {
                    var,
                    value: value_index(domain, *value),
                    next,
                }
            }
            StmtKind::Add { var, amount } => Op::Add {
                var: var_id(*var),
                amount: u32::from(*amount),
                next,
            },
            StmtKind::Sub { var, amount } => Op::Sub {
                var: var_id(*var),
                amount: u32::from(*amount),
                next,
            },
            StmtKind::MetaSet { .. } => Op::MetaWrite { next },
        };
        self.push(op, span)
    }

    /// Places the pre-allocated label node in front of `next`. A label statement seen
    /// twice (a duplicate, already reported by stage 2) gets a fresh node.
    fn label(&mut self, index: u32, span: Span, next: NodeId) -> NodeId {
        let op = Op::Jump {
            next,
            from_goto: false,
        };
        let slot = usize::try_from(index).ok();
        let placed = slot.and_then(|i| self.placed.get_mut(i));
        let node = slot.and_then(|i| self.labels.get(i).copied());
        match (placed, node) {
            (Some(placed), Some(node)) if !*placed => {
                *placed = true;
                let node_slot = usize::try_from(node.0)
                    .ok()
                    .and_then(|i| self.nodes.get_mut(i));
                if let Some(slot) = node_slot {
                    slot.op = op;
                    slot.span = span;
                }
                node
            }
            _ => self.push(op, span),
        }
    }

    fn match_vars<B>(&self, m: &MatchStmt<B>) -> (Vec<VarId>, Vec<Domain>) {
        m.vars.iter().map(|&v| self.var(v)).unzip()
    }

    /// Flattens choice items into arms, recording each enclosing `<match>` as an
    /// [`IrChoiceMatch`] and the selected match arms as the arm's guard.
    fn choice_items(
        &mut self,
        items: &[ChoiceItem],
        guard: &mut Vec<(u32, u32)>,
        matches: &mut Vec<IrChoiceMatch>,
        arms: &mut Vec<IrArm>,
        next: NodeId,
    ) {
        for item in items {
            match item {
                ChoiceItem::Arm(arm) => {
                    let target = self.block(&arm.body, next);
                    arms.push(IrArm {
                        id: arm.id.clone(),
                        guard: guard.clone(),
                        target,
                    });
                }
                ChoiceItem::Match(m) => {
                    let (vars, domains) = self.match_vars(m);
                    let match_index = u32::try_from(matches.len()).unwrap_or(u32::MAX);
                    matches.push(IrChoiceMatch {
                        vars,
                        arms: m
                            .arms
                            .iter()
                            .map(|arm| lower_rows(&domains, &arm.rows))
                            .collect(),
                    });
                    for (arm_index, arm) in m.arms.iter().enumerate() {
                        let arm_index = u32::try_from(arm_index).unwrap_or(u32::MAX);
                        guard.push((match_index, arm_index));
                        self.choice_items(&arm.body, guard, matches, arms, next);
                        guard.pop();
                    }
                }
            }
        }
    }
}

fn lower_rows(domains: &[Domain], rows: &[Vec<Pattern>]) -> Vec<Vec<IrPattern>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .zip(domains)
                .map(|(&pattern, &domain)| lower_pattern(domain, pattern))
                .collect()
        })
        .collect()
}

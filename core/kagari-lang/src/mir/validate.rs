//! Structural checks on a MIR body; violations are compiler bugs.
//!
//! 1. `Step`/`Rng`/`SceneCall` appear only in `Scene` bodies.
//! 2. `SceneCall` callees are scenes, and a call statement never targets a scene.
//! 3. Call kinds follow the kind rules of `docs/dev/src/formats/kagari.md#verification`,
//!    with `Const` bodies treated as `const fn`.
//! 4. Story variables appear only in `Instant`/`Scene` bodies.
//! 5. Every referenced local and block exists, and `arg_count < locals.len()`.

use crate::callee::{self, CalleeKind};
use crate::db::Db;
use crate::input::Program;
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::ty::{FnDefId, FnRef, Ty, TyKind};

use super::{
    BasicBlock, Body, BodyKind, Callee, Local, Operand, Place, PlaceBase, Proj, Rvalue,
    StatementKind, Terminator,
};

/// The violations in `body`, one message each; empty when it is well formed.
pub(crate) fn validate(db: &dyn Db, program: Program, body: &Body) -> Vec<String> {
    let mut cx = Validator {
        sigs: program_signatures(db, program),
        body,
        errors: Vec::new(),
        at: String::new(),
    };
    cx.run();
    cx.errors
}

struct Validator<'a> {
    sigs: &'a ProgramSignatures,
    body: &'a Body,
    errors: Vec<String>,
    /// Where the current check is, for messages.
    at: String,
}

impl Validator<'_> {
    fn error(&mut self, message: String) {
        self.errors.push(format!("{}: {message}", self.at));
    }

    fn run(&mut self) {
        let body = self.body;
        if body.arg_count as usize >= body.locals.len() {
            self.at = "body".to_owned();
            self.error(format!(
                "arg_count {} but only {} locals",
                body.arg_count,
                body.locals.len()
            ));
        }
        self.at = "entry".to_owned();
        self.block(body.entry);
        for (block, data) in body.blocks.iter() {
            for (index, stmt) in data.stmts.iter().enumerate() {
                self.at = format!("bb{}[{index}]", block.into_raw().into_u32());
                match &stmt.kind {
                    StatementKind::Assign(place, rvalue) => {
                        self.place(place);
                        self.rvalue(rvalue);
                    }
                    StatementKind::Call { dest, callee, args } => {
                        self.place(dest);
                        self.operands(args);
                        self.callee(callee);
                        match self.callee_kind(callee) {
                            Some(
                                kind @ (CalleeKind::Scene | CalleeKind::Step | CalleeKind::Rng),
                            ) => {
                                self.error(format!("call statement targets a {kind:?} callee"));
                            }
                            Some(kind) => self.kind_rule(kind),
                            None => {}
                        }
                    }
                }
            }
            self.at = format!("bb{} terminator", block.into_raw().into_u32());
            for succ in data.term.successors() {
                self.block(succ);
            }
            if data.term.is_suspension() && body.kind != BodyKind::Scene {
                self.error(format!("suspension terminator in a {:?} body", body.kind));
            }
            match &data.term {
                Terminator::Goto(_) | Terminator::Return | Terminator::Unreachable => {}
                Terminator::SwitchBool {
                    cond,
                    short_circuit,
                    ..
                } => {
                    self.operand(cond);
                    if let Some(local) = short_circuit {
                        self.local(*local);
                    }
                }
                Terminator::SwitchVariant { place, .. } => self.place(place),
                Terminator::Step { arg, dest, .. } => {
                    self.operand(arg);
                    self.place(dest);
                }
                Terminator::Rng { dest, .. } => self.place(dest),
                Terminator::SceneCall {
                    callee, args, dest, ..
                } => {
                    self.callee(callee);
                    self.operands(args);
                    self.place(dest);
                    match self.callee_kind(callee) {
                        Some(CalleeKind::Scene) => {}
                        kind => self.error(format!("scene call of a {kind:?} callee")),
                    }
                }
            }
        }
    }

    /// Check 3 for a call statement of `kind` (scene calls are covered by checks 1 and 2).
    fn kind_rule(&mut self, kind: CalleeKind) {
        let allowed = match self.body.kind {
            BodyKind::Fn => matches!(kind, CalleeKind::Plain | CalleeKind::Const),
            BodyKind::ConstFn | BodyKind::Const => kind == CalleeKind::Const,
            BodyKind::Instant | BodyKind::Scene => {
                matches!(kind, CalleeKind::Instant | CalleeKind::Const)
            }
        };
        if !allowed {
            self.error(format!(
                "a {:?} body may not call a {kind:?} callee",
                self.body.kind
            ));
        }
    }

    /// The kind of a callee; `None` when it is unknown (a value of a type the validator
    /// cannot compute). Constructors count as `const fn`.
    fn callee_kind(&self, callee: &Callee) -> Option<CalleeKind> {
        match callee {
            Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => self.fn_kind(fn_ref),
            Callee::Value(operand) => {
                let ty = self.operand_ty(operand)?;
                callee::classify(self.sigs, &callee::Callee::Value(ty))
            }
        }
    }

    fn fn_kind(&self, fn_ref: &FnRef) -> Option<CalleeKind> {
        match fn_ref.def {
            FnDefId::Item(item) => callee::classify(self.sigs, &callee::Callee::Item(item)),
            FnDefId::StructCtor(_) | FnDefId::VariantCtor(..) => Some(CalleeKind::Const),
        }
    }

    /// The type of an operand when it is a copy of a local through derefs and tuple fields.
    fn operand_ty(&self, operand: &Operand) -> Option<Ty> {
        let Operand::Copy(place) = operand else {
            return None;
        };
        let PlaceBase::Local(local) = place.base else {
            return None;
        };
        let mut ty = self.body.local_decl(local)?.ty.clone();
        for proj in &place.proj {
            ty = match (proj, ty.kind()) {
                (Proj::Deref, TyKind::Ref { inner, .. }) => inner.clone(),
                (Proj::Field(index), TyKind::Tuple(elems)) => elems.get(*index as usize)?.clone(),
                _ => return None,
            };
        }
        Some(ty)
    }

    fn callee(&mut self, callee: &Callee) {
        if let Callee::Value(operand) = callee {
            self.operand(operand);
        }
    }

    fn rvalue(&mut self, rvalue: &Rvalue) {
        match rvalue {
            Rvalue::Use(operand)
            | Rvalue::IntNeg { operand, .. }
            | Rvalue::IntNot { operand, .. }
            | Rvalue::IntClamp { operand, .. }
            | Rvalue::BoolNot(operand)
            | Rvalue::DecNeg(operand)
            | Rvalue::RatNeg(operand)
            | Rvalue::Repeat { operand, .. }
            | Rvalue::Cast { operand, .. } => self.operand(operand),
            Rvalue::IntBin { lhs, rhs, .. }
            | Rvalue::BoolBin { lhs, rhs, .. }
            | Rvalue::DecBin { lhs, rhs, .. }
            | Rvalue::RatBin { lhs, rhs, .. }
            | Rvalue::Cmp { lhs, rhs, .. } => {
                self.operand(lhs);
                self.operand(rhs);
            }
            Rvalue::Aggregate { ops, .. } | Rvalue::Closure { captures: ops, .. } => {
                self.operands(ops);
            }
            Rvalue::Ref { place, .. } | Rvalue::Len(place) => self.place(place),
        }
    }

    fn operands(&mut self, operands: &[Operand]) {
        for operand in operands {
            self.operand(operand);
        }
    }

    fn operand(&mut self, operand: &Operand) {
        if let Operand::Copy(place) = operand {
            self.place(place);
        }
    }

    fn place(&mut self, place: &Place) {
        match place.base {
            PlaceBase::Local(local) => self.local(local),
            PlaceBase::StoryVar(_) => {
                if !matches!(self.body.kind, BodyKind::Instant | BodyKind::Scene) {
                    self.error(format!(
                        "story variable used in a {:?} body",
                        self.body.kind
                    ));
                }
            }
        }
        for proj in &place.proj {
            if let Proj::Index(local) = proj {
                self.local(*local);
            }
        }
    }

    fn local(&mut self, local: Local) {
        if self.body.local_decl(local).is_none() {
            self.error(format!(
                "local _{} does not exist",
                local.into_raw().into_u32()
            ));
        }
    }

    fn block(&mut self, block: BasicBlock) {
        if self.body.block(block).is_none() {
            self.error(format!(
                "block bb{} does not exist",
                block.into_raw().into_u32()
            ));
        }
    }
}

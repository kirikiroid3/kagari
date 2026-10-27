//! Terminators: jumps (omitted when the target is the next block), switches, returns and
//! the story's suspensions.

use kagari_bytecode::Instr;

use crate::mir::{BasicBlock, Body, Place, Terminator};
use crate::ty::GenericArg;

use super::{FnCodegen, block_label, reg};

impl FnCodegen<'_, '_> {
    /// Lowers `term`; `next` is the block emitted after this one.
    pub(super) fn terminator(&mut self, term: &Terminator, next: Option<BasicBlock>) {
        match term {
            Terminator::Goto(target) => self.goto(*target, next),
            Terminator::SwitchBool {
                cond, then, other, ..
            } => {
                let cond = reg(self.operand_auto(cond));
                if next == Some(*then) {
                    self.emit(Instr::JmpNot {
                        cond,
                        target: block_label(*other),
                    });
                } else if next == Some(*other) {
                    self.emit(Instr::JmpIf {
                        cond,
                        target: block_label(*then),
                    });
                } else {
                    self.emit(Instr::JmpNot {
                        cond,
                        target: block_label(*other),
                    });
                    self.jmp(*then);
                }
            }
            Terminator::SwitchVariant {
                place,
                adt,
                targets,
                otherwise,
            } => {
                let src = reg(self.read_auto(place));
                let ty = self.em.type_id(*adt);
                let c = self.temp();
                let test = |this: &mut Self, variant: u32| {
                    this.emit(Instr::IsVariant {
                        dst: reg(c),
                        src,
                        ty,
                        variant,
                    });
                };
                // The target laid out next is reached without a jump.
                let fall = targets.iter().position(|(_, block)| Some(*block) == next);
                // Without `otherwise` one target (the next block, else the last) needs no
                // test.
                let untested = match otherwise {
                    Some(_) => fall,
                    None => fall.or(targets.len().checked_sub(1)),
                };
                for (position, (variant, target)) in targets.iter().enumerate() {
                    if Some(position) == untested {
                        continue;
                    }
                    test(self, *variant);
                    self.emit(Instr::JmpIf {
                        cond: reg(c),
                        target: block_label(*target),
                    });
                }
                match (
                    otherwise,
                    untested.and_then(|position| targets.get(position)),
                ) {
                    (Some(other), Some((variant, _))) => {
                        test(self, *variant);
                        self.emit(Instr::JmpNot {
                            cond: reg(c),
                            target: block_label(*other),
                        });
                    }
                    (Some(other), None) => self.goto(*other, next),
                    (None, Some((_, target))) => self.goto(*target, next),
                    (None, None) => self.emit(Instr::Unreachable),
                }
            }
            Terminator::Return => {
                let src = self.read_raw(&Place::local(Body::RETURN));
                self.emit(Instr::Ret { src: reg(src) });
            }
            Terminator::Unreachable => self.emit(Instr::Unreachable),
            Terminator::Step {
                arg,
                dest,
                next: target,
            } => {
                let record = reg(self.operand_auto(arg));
                let dst = self.dest(dest);
                self.emit(Instr::Step {
                    dst: reg(dst),
                    record,
                });
                self.finish_dest(dest, dst);
                self.goto(*target, next);
            }
            Terminator::Rng {
                ty,
                dest,
                next: target,
            } => {
                let domain = self.rand_domain(Some(&GenericArg::Type(ty.clone())));
                let dst = self.dest(dest);
                match domain {
                    Some(domain) => self.emit(Instr::Rand {
                        dst: reg(dst),
                        domain,
                    }),
                    None => self.emit(Instr::Unreachable),
                }
                self.finish_dest(dest, dst);
                self.goto(*target, next);
            }
            Terminator::SceneCall {
                callee,
                args,
                dest,
                next: target,
            } => {
                self.call_stmt(dest, callee, args);
                self.goto(*target, next);
            }
        }
    }

    fn goto(&mut self, target: BasicBlock, next: Option<BasicBlock>) {
        if next != Some(target) {
            self.jmp(target);
        }
    }
}

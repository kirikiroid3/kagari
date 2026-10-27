//! Assignments: rvalues, operands and constants.

use kagari_bytecode::{ArithOp, BoolBinOp, CmpOp, Constant, Instr, IntBinOp, IntUnOp};

use crate::eval::machine::value::{subst_arg, subst_fn_ref, subst_ty};
use crate::eval::queries::{StaticValue, static_value};
use crate::eval::{ConstValue, const_value, promoted_value};
use crate::hir::BinaryOp;
use crate::mir::{AggregateKind, ConstOperand, Operand, Place, Rvalue};
use crate::ty::{Const, GenericArg, GenericArgs, Ty};

use super::super::{Instance, contains_screen};
use super::{FnCodegen, R, count, nth, reg};

fn int_op(op: BinaryOp) -> Option<IntBinOp> {
    Some(match op {
        BinaryOp::Add => IntBinOp::Add,
        BinaryOp::Sub => IntBinOp::Sub,
        BinaryOp::Mul => IntBinOp::Mul,
        BinaryOp::Div => IntBinOp::Div,
        BinaryOp::Rem => IntBinOp::Rem,
        BinaryOp::BitAnd => IntBinOp::And,
        BinaryOp::BitOr => IntBinOp::Or,
        BinaryOp::BitXor => IntBinOp::Xor,
        BinaryOp::Shl => IntBinOp::Shl,
        BinaryOp::Shr => IntBinOp::Shr,
        _ => return None,
    })
}

fn arith_op(op: BinaryOp) -> Option<ArithOp> {
    Some(match op {
        BinaryOp::Add => ArithOp::Add,
        BinaryOp::Sub => ArithOp::Sub,
        BinaryOp::Mul => ArithOp::Mul,
        BinaryOp::Div => ArithOp::Div,
        _ => return None,
    })
}

fn bool_op(op: BinaryOp) -> Option<BoolBinOp> {
    Some(match op {
        BinaryOp::BitAnd => BoolBinOp::And,
        BinaryOp::BitOr => BoolBinOp::Or,
        BinaryOp::BitXor => BoolBinOp::Xor,
        _ => return None,
    })
}

fn cmp_op(op: BinaryOp) -> Option<CmpOp> {
    Some(match op {
        BinaryOp::Eq => CmpOp::Eq,
        BinaryOp::Ne => CmpOp::Ne,
        BinaryOp::Lt => CmpOp::Lt,
        BinaryOp::Le => CmpOp::Le,
        BinaryOp::Gt => CmpOp::Gt,
        BinaryOp::Ge => CmpOp::Ge,
        _ => return None,
    })
}

impl FnCodegen<'_, '_> {
    pub(super) fn assign(&mut self, place: &Place, rvalue: &Rvalue) {
        let dst = self.dest(place);
        self.rvalue_into(rvalue, dst);
        self.finish_dest(place, dst);
    }

    /// Computes `rvalue` into `dst`; only the last instruction writes `dst`.
    fn rvalue_into(&mut self, rvalue: &Rvalue, dst: R) {
        let d = reg(dst);
        match rvalue {
            Rvalue::Use(operand) => self.operand_into(operand, dst),
            Rvalue::IntBin { op, lhs, rhs, ty } => {
                let (lhs, rhs) = (self.operand_auto(lhs), self.operand_auto(rhs));
                let ty = self.int_of(ty);
                self.emit(match int_op(*op) {
                    Some(op) => Instr::IntBin {
                        op,
                        dst: d,
                        lhs: reg(lhs),
                        rhs: reg(rhs),
                        ty,
                    },
                    None => Instr::Unreachable,
                });
            }
            Rvalue::IntNeg { operand, ty } => self.int_un(IntUnOp::Neg, operand, ty, dst),
            Rvalue::IntNot { operand, ty } => self.int_un(IntUnOp::Not, operand, ty, dst),
            Rvalue::IntClamp { operand, ty } => self.int_un(IntUnOp::Clamp, operand, ty, dst),
            Rvalue::BoolNot(operand) => {
                let src = reg(self.operand_auto(operand));
                self.emit(Instr::Not { dst: d, src });
            }
            Rvalue::BoolBin { op, lhs, rhs } => {
                let (lhs, rhs) = (reg(self.operand_auto(lhs)), reg(self.operand_auto(rhs)));
                self.emit(match bool_op(*op) {
                    Some(op) => Instr::BoolBin {
                        op,
                        dst: d,
                        lhs,
                        rhs,
                    },
                    None => Instr::Unreachable,
                });
            }
            Rvalue::DecBin { op, lhs, rhs } => {
                let (lhs, rhs) = (reg(self.operand_auto(lhs)), reg(self.operand_auto(rhs)));
                self.emit(match arith_op(*op) {
                    Some(op) => Instr::DecBin {
                        op,
                        dst: d,
                        lhs,
                        rhs,
                    },
                    None => Instr::Unreachable,
                });
            }
            Rvalue::RatBin { op, lhs, rhs } => {
                let (lhs, rhs) = (reg(self.operand_auto(lhs)), reg(self.operand_auto(rhs)));
                self.emit(match arith_op(*op) {
                    Some(op) => Instr::RatBin {
                        op,
                        dst: d,
                        lhs,
                        rhs,
                    },
                    None => Instr::Unreachable,
                });
            }
            Rvalue::DecNeg(operand) => {
                let src = reg(self.operand_auto(operand));
                self.emit(Instr::DecNeg { dst: d, src });
            }
            Rvalue::RatNeg(operand) => {
                let src = reg(self.operand_auto(operand));
                self.emit(Instr::RatNeg { dst: d, src });
            }
            Rvalue::Cmp { op, lhs, rhs } => {
                let (lhs, rhs) = (reg(self.operand_auto(lhs)), reg(self.operand_auto(rhs)));
                self.emit(match cmp_op(*op) {
                    Some(op) => Instr::Cmp {
                        op,
                        dst: d,
                        lhs,
                        rhs,
                    },
                    None => Instr::Unreachable,
                });
            }
            Rvalue::Aggregate { kind, ops } => {
                let first = self.window_operands(ops);
                let n = count(ops.len());
                let first = reg(first);
                let instr = match kind {
                    AggregateKind::Tuple => Instr::Tuple {
                        dst: d,
                        first,
                        count: n,
                    },
                    AggregateKind::Array => Instr::Array {
                        dst: d,
                        first,
                        count: n,
                    },
                    AggregateKind::Adt(ctor) => Instr::Adt {
                        dst: d,
                        first,
                        count: n,
                        ty: self.em.type_id(ctor.adt),
                        variant: ctor.variant.unwrap_or(0),
                    },
                };
                self.emit(instr);
            }
            Rvalue::Repeat { operand, count } => {
                let value = reg(self.operand_raw(operand));
                self.emit(Instr::Repeat {
                    dst: d,
                    value,
                    count: *count,
                });
            }
            Rvalue::Ref { place, .. } => self.reference(place, dst),
            Rvalue::Len(place) => {
                let src = reg(self.read_auto(place));
                self.emit(Instr::Len { dst: d, src });
            }
            Rvalue::Closure { expr, captures } => self.closure(*expr, captures, dst),
            // Casts have no instruction yet.
            Rvalue::Cast { .. } => self.emit(Instr::Unreachable),
        }
    }

    fn int_un(&mut self, op: IntUnOp, operand: &Operand, ty: &Ty, dst: R) {
        let src = reg(self.operand_auto(operand));
        let ty = self.int_of(ty);
        self.emit(Instr::IntUn {
            op,
            dst: reg(dst),
            src,
            ty,
        });
    }

    /// The int type of `ty` in this instance.
    pub(super) fn int_of(&mut self, ty: &Ty) -> kagari_bytecode::IntTypeId {
        let ty = subst_ty(ty, &self.env);
        self.em.int_of(&ty)
    }

    /// Moves `operands` into the argument window; returns its first register.
    pub(super) fn window_operands(&mut self, operands: &[Operand]) -> R {
        let first = self.frame.window;
        for (index, operand) in operands.iter().enumerate() {
            self.operand_into(operand, nth(first, index));
        }
        first
    }

    // ----- Operands -----------------------------------------------------------------

    /// A register holding the operand or a reference to it, for an operand the VM
    /// dereferences itself.
    pub(super) fn operand_auto(&mut self, operand: &Operand) -> R {
        match operand {
            Operand::Copy(place) => self.read_auto(place),
            Operand::Const(constant) => {
                let t = self.temp();
                self.constant_into(constant, t);
                t
            }
        }
    }

    /// A register holding the operand's exact value.
    pub(super) fn operand_raw(&mut self, operand: &Operand) -> R {
        match operand {
            Operand::Copy(place) => self.read_raw(place),
            Operand::Const(constant) => {
                let t = self.temp();
                self.constant_into(constant, t);
                t
            }
        }
    }

    pub(super) fn operand_into(&mut self, operand: &Operand, dst: R) {
        match operand {
            Operand::Copy(place) => self.read_into(place, dst),
            Operand::Const(constant) => self.constant_into(constant, dst),
        }
    }

    // ----- Constants ----------------------------------------------------------------

    pub(super) fn load_value(&mut self, dst: R, value: &ConstValue) {
        match value {
            ConstValue::Bool(value) => self.emit(Instr::Bool {
                dst: reg(dst),
                value: *value,
            }),
            ConstValue::Int(value) if let Ok(value) = i64::try_from(*value) => {
                self.emit(Instr::Int {
                    dst: reg(dst),
                    value,
                });
            }
            ConstValue::Tuple(items) if items.is_empty() => self.emit(Instr::Tuple {
                dst: reg(dst),
                first: 0,
                count: 0,
            }),
            _ => match self.em.constant(value) {
                Some(id) => self.emit(Instr::Const {
                    dst: reg(dst),
                    value: id,
                }),
                None => self.emit(Instr::Unreachable),
            },
        }
    }

    /// Calls the initializer function of a constant whose value holds a `Screen`.
    fn call_init(&mut self, instance: Instance, dst: R) {
        let func = self.em.request(instance);
        self.emit(Instr::Call {
            dst: reg(dst),
            args: 0,
            argc: 0,
            func,
        });
    }

    fn constant_into(&mut self, constant: &ConstOperand, dst: R) {
        let (db, program) = (self.em.db, self.em.program);
        match constant {
            ConstOperand::Value(value) => self.load_value(dst, value),
            ConstOperand::Item { item, args } => {
                let args: GenericArgs = args.iter().map(|arg| subst_arg(arg, &self.env)).collect();
                match const_value(db, program, *item, args.clone()) {
                    Ok(value) if !contains_screen(value) => self.load_value(dst, value),
                    Ok(_) => self.call_init(Instance::ConstInit(*item, args.to_vec()), dst),
                    Err(error) => {
                        self.eval_error(error);
                        self.emit(Instr::Unreachable);
                    }
                }
            }
            ConstOperand::Param { owner, index } => match self.env.arg(*owner, *index) {
                Some(GenericArg::Const(Const::Value(value))) => {
                    let value = ConstValue::Int(*value);
                    self.load_value(dst, &value);
                }
                _ => self.emit(Instr::Unreachable),
            },
            ConstOperand::Promoted(index) => {
                let value = self
                    .body
                    .map(|body| promoted_value(db, program, body.source.owner(), *index));
                match value {
                    Some(Ok(value)) if !contains_screen(value) => self.load_value(dst, value),
                    // A value with a `Screen`, or one that faults: computed at run time.
                    _ => {
                        let owner = Box::new(self.root.clone());
                        self.call_init(Instance::Promoted(owner, *index), dst);
                    }
                }
            }
            ConstOperand::Fn(fn_ref) => {
                let fn_ref = subst_fn_ref(fn_ref, &self.env);
                let func = self.em.func_value(&fn_ref);
                self.emit(Instr::Func {
                    dst: reg(dst),
                    func,
                });
            }
            ConstOperand::Handle(item) => self.handle(*item, dst),
            ConstOperand::Static(item) => match static_value(db, program, *item) {
                Ok(StaticValue {
                    handle: Some(_), ..
                }) => self.handle(*item, dst),
                Ok(StaticValue { value, .. }) if !contains_screen(value) => {
                    self.load_value(dst, value);
                }
                Ok(_) => self.call_init(Instance::ConstInit(*item, Vec::new()), dst),
                Err(error) => {
                    self.eval_error(error);
                    self.emit(Instr::Unreachable);
                }
            },
        }
    }

    /// The handle of a `Setting`/`Meta` static.
    fn handle(&mut self, item: crate::item_tree::ItemId, dst: R) {
        match self.em.statics.get(&item).copied() {
            Some(id) => {
                let constant = self.em.intern(Constant::Handle(id));
                self.emit(Instr::Const {
                    dst: reg(dst),
                    value: constant,
                });
            }
            None => self.emit(Instr::Unreachable),
        }
    }
}

//! Calls: direct calls, builtins as intrinsics or host calls, calls of function values,
//! closures, and the wrapper functions of constructors and builtins used as values.

use kagari_bytecode::{CmpOp, FnKind as BcFnKind, Instr, IntUnOp, RandDomain};
use smallvec::SmallVec;

use crate::eval::machine::value::{GenericEnv, subst_const, subst_fn_ref, subst_ty};
use crate::eval::std_items::Builtin;
use crate::eval::{AssetKind, ConstValue, Text};
use crate::hir::{ClosureKind, ExprId, FnPtrKind};
use crate::item_tree::ItemId;
use crate::mir::{Callee, ConstOperand, Operand, Place};
use crate::ty::{Const, FnDefId, FnRef, GenericArg, Ty, TyKind};

use super::super::{Emitter, Instance, int_type_of};
use super::place::is_ref;
use super::{FnCodegen, Lowered, R, count, nth, reg};

/// The constant path of an `asset("…")` argument.
fn asset_path(operand: Option<&Operand>) -> Option<&str> {
    match operand? {
        Operand::Const(ConstOperand::Value(
            ConstValue::String(path) | ConstValue::Text(Text::Plain(path)),
        )) => Some(path),
        _ => None,
    }
}

impl FnCodegen<'_, '_> {
    /// A call statement or a scene call: `dest = callee(args)`.
    pub(super) fn call_stmt(&mut self, dest: &Place, callee: &Callee, args: &[Operand]) {
        match callee {
            Callee::Value(operand) => {
                let kind = self.value_kind(operand);
                let target = self.callee_value(operand);
                let first = self.window_operands(args);
                let dst = self.dest(dest);
                self.emit(Instr::CallVal {
                    dst: reg(dst),
                    args: reg(first),
                    argc: count(args.len()),
                    callee: reg(target),
                    kind,
                });
                self.finish_dest(dest, dst);
            }
            Callee::Fn(fn_ref) | Callee::Builtin(_, fn_ref) => {
                let fn_ref = subst_fn_ref(fn_ref, &self.env);
                let builtin = match fn_ref.def {
                    FnDefId::Item(item) => self.em.std.builtin(item).map(|b| (item, b)),
                    _ => None,
                };
                let ret = self.ret_ty(dest, &fn_ref);
                match builtin {
                    // Intrinsics read their operands where they are.
                    Some((item, builtin)) => {
                        let mut regs: SmallVec<[R; 4]> = SmallVec::new();
                        for arg in args {
                            regs.push(self.operand_raw(arg));
                        }
                        let dst = self.dest(dest);
                        let path = asset_path(args.first());
                        self.builtin(item, builtin, &fn_ref, &regs, dst, &ret, path);
                        self.finish_dest(dest, dst);
                    }
                    None => {
                        let first = self.window_operands(args);
                        let dst = self.dest(dest);
                        self.direct_call(&fn_ref, first, args.len(), dst);
                        self.finish_dest(dest, dst);
                    }
                }
            }
        }
    }

    /// The function value a `Callee::Value` operand calls, dereferenced.
    fn callee_value(&mut self, operand: &Operand) -> R {
        if let Operand::Copy(place) = operand
            && self.place_ty(place).is_some_and(|ty| is_ref(&ty))
        {
            let src = self.read_auto(place);
            let t = self.temp();
            self.emit(Instr::Load {
                dst: reg(t),
                src: reg(src),
            });
            return t;
        }
        self.operand_raw(operand)
    }

    /// The kind a function value has.
    fn value_kind(&self, operand: &Operand) -> BcFnKind {
        let ty = match operand {
            Operand::Copy(place) => self.place_ty(place),
            Operand::Const(ConstOperand::Fn(fn_ref)) => {
                return match fn_ref.def {
                    FnDefId::Item(item) => self.em.fn_kind(item),
                    _ => BcFnKind::Const,
                };
            }
            Operand::Const(_) => None,
        };
        let mut ty = ty.unwrap_or_else(Ty::error);
        while let TyKind::Ref { inner, .. } = ty.kind() {
            ty = inner.clone();
        }
        match ty.kind() {
            TyKind::FnPtr { kind, .. } => match kind {
                FnPtrKind::Plain => BcFnKind::Fn,
                FnPtrKind::Const => BcFnKind::Const,
                FnPtrKind::Pushdown => BcFnKind::Instant,
                FnPtrKind::Scene => BcFnKind::Scene,
            },
            TyKind::Closure { kind, .. } => match kind {
                ClosureKind::Plain => BcFnKind::Fn,
                ClosureKind::Const => BcFnKind::Const,
                ClosureKind::Pushdown => BcFnKind::Instant,
            },
            TyKind::FnDef(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => self.em.fn_kind(item),
                _ => BcFnKind::Const,
            },
            _ => BcFnKind::Fn,
        }
    }

    /// The result type of a call of `fn_ref` writing `dest`.
    fn ret_ty(&self, dest: &Place, fn_ref: &FnRef) -> Ty {
        if let Some(ty) = self.place_ty(dest) {
            return ty;
        }
        let FnDefId::Item(item) = fn_ref.def else {
            return Ty::error();
        };
        match self.em.sigs.fn_sig(item) {
            Some(sig) => subst_ty(
                &sig.ret,
                &GenericEnv::for_item(self.em.sigs, item, fn_ref.args.clone()),
            ),
            None => Ty::error(),
        }
    }

    /// A direct call of a function with a body or a constructor, with the arguments in
    /// `first..first + n`.
    fn direct_call(&mut self, fn_ref: &FnRef, first: R, n: usize, dst: R) {
        let instr = match fn_ref.def {
            FnDefId::StructCtor(item) | FnDefId::VariantCtor(item, _) => {
                let variant = match fn_ref.def {
                    FnDefId::VariantCtor(_, variant) => variant,
                    _ => 0,
                };
                Instr::Adt {
                    dst: reg(dst),
                    first: reg(first),
                    count: count(n),
                    ty: self.em.type_id(item),
                    variant,
                }
            }
            FnDefId::Item(_) => Instr::Call {
                dst: reg(dst),
                args: reg(first),
                argc: count(n),
                func: self.em.request(Instance::Fn(fn_ref.clone())),
            },
        };
        self.emit(instr);
    }

    /// A builtin with its operands in `args`: an intrinsic instruction, or a call of the
    /// engine's import.
    #[allow(clippy::too_many_arguments)]
    fn builtin(
        &mut self,
        item: ItemId,
        builtin: Builtin,
        fn_ref: &FnRef,
        args: &[R],
        dst: R,
        ret: &Ty,
        asset: Option<&str>,
    ) {
        let std = self.em.std;
        let arg = |index: usize| reg(args.get(index).copied().unwrap_or(0));
        let (d, a, b1, b2) = (reg(dst), arg(0), arg(1), arg(2));
        if item == std.push_step {
            // Only the wrapper of `push_step` used as a value gets here: its one step.
            self.emit(Instr::Step {
                dst: d,
                record: a,
                id: 0,
            });
            return;
        }
        if item == std.rng_choose
            && let Some(domain) = self.rand_domain(fn_ref.args.first())
        {
            self.emit(Instr::Rand { dst: d, domain });
            return;
        }
        if item == std.ui_slider {
            let instr = self.slider_call(item, fn_ref, args, dst);
            self.emit(instr);
            return;
        }
        let instr = match builtin {
            Builtin::Asset => {
                let asset_item = match fn_ref.args.last() {
                    Some(GenericArg::Type(ty)) if let TyKind::Adt { item, .. } = ty.kind() => {
                        Some(*item)
                    }
                    _ => match ret.kind() {
                        TyKind::Adt { item, .. } => Some(*item),
                        _ => None,
                    },
                };
                let kind = asset_item.and_then(|item| std.asset_kind(item));
                match (asset, kind) {
                    (Some(path), Some(kind)) => {
                        // Assets declared in std are std files; others lie in the asset
                        // directory.
                        let in_std = self.body.is_some_and(|body| {
                            matches!(
                                body.source.owner().file.path(self.em.db),
                                crate::input::FilePath::Std(_)
                            )
                        });
                        let file = if in_std {
                            crate::input::FilePath::Std(path.into())
                        } else {
                            self.em.program.asset_root(self.em.db).join(path)
                        };
                        let value = ConstValue::Asset { kind, path: file };
                        self.load_value(dst, &value);
                        return;
                    }
                    _ => self.import_call(item, args, dst),
                }
            }
            Builtin::SystemFont => {
                let file = asset
                    .filter(|path| crate::assets::stays_inside(path))
                    .and_then(|path| {
                        crate::assets::system_font_file(self.em.db, self.em.program, path)
                    });
                match file {
                    Some(file) => {
                        let value = ConstValue::Asset {
                            kind: AssetKind::Font,
                            path: file,
                        };
                        self.load_value(dst, &value);
                        return;
                    }
                    None => self.import_call(item, args, dst),
                }
            }
            // The erasure intrinsics only forget a type.
            Builtin::ShaderErase | Builtin::ShaderArgsErase => Instr::Move { dst: d, src: a },
            Builtin::VecNew => Instr::VecNew { dst: d },
            Builtin::VecLen => Instr::Len { dst: d, src: a },
            Builtin::VecIsEmpty => {
                let len = self.temp();
                let zero = self.temp();
                self.emit(Instr::Len {
                    dst: reg(len),
                    src: a,
                });
                self.emit(Instr::Int {
                    dst: reg(zero),
                    value: 0,
                });
                Instr::Cmp {
                    op: CmpOp::Eq,
                    dst: d,
                    lhs: reg(len),
                    rhs: reg(zero),
                }
            }
            Builtin::VecGet => Instr::VecGet {
                dst: d,
                vec: a,
                index: b1,
                option: self.em.type_id(std.option.item),
            },
            Builtin::VecSet => {
                self.emit(Instr::VecSet {
                    vec: a,
                    index: b1,
                    value: b2,
                });
                Instr::Tuple {
                    dst: d,
                    first: 0,
                    count: 0,
                }
            }
            Builtin::VecPush => {
                self.emit(Instr::VecPush { vec: a, value: b1 });
                Instr::Tuple {
                    dst: d,
                    first: 0,
                    count: 0,
                }
            }
            Builtin::IntegerInto if std.targets_decimal(fn_ref) => Instr::IntDec { dst: d, src: a },
            Builtin::IntegerInto => {
                self.mov(dst, args.first().copied().unwrap_or(0));
                return;
            }
            Builtin::IntegerSaturate => {
                let mut ty = ret.clone();
                while let TyKind::Ref { inner, .. } = ty.kind() {
                    ty = inner.clone();
                }
                Instr::IntUn {
                    op: IntUnOp::Clamp,
                    dst: d,
                    src: a,
                    ty: self.em.int_of(&ty),
                }
            }
            Builtin::Ruby => Instr::Ruby {
                dst: d,
                base: a,
                reading: b1,
            },
            Builtin::MonoRuby => Instr::MonoRuby {
                dst: d,
                base: a,
                readings: b1,
            },
            Builtin::Effect => Instr::Effect {
                dst: d,
                base: a,
                effect: b1,
            },
            Builtin::Concat => Instr::Concat { dst: d, parts: a },
            Builtin::ToText => Instr::ToText { dst: d, src: a },
            Builtin::RatNew => Instr::RatNew {
                dst: d,
                num: a,
                den: b1,
            },
            Builtin::RatToDecimal => Instr::RatDec { dst: d, src: a },
            Builtin::DecimalFromInt => Instr::IntDec { dst: d, src: a },
            _ => self.import_call(item, args, dst),
        };
        self.emit(instr);
    }

    /// A host call: its arguments must be consecutive registers, so they are moved into the
    /// argument window unless they already are.
    fn import_call(&mut self, item: ItemId, args: &[R], dst: R) -> Instr {
        let consecutive = args
            .iter()
            .zip(args.iter().skip(1))
            .all(|(a, b)| a.checked_add(1) == Some(*b));
        let first = match args.first() {
            Some(first) if consecutive => *first,
            _ => {
                let window = self.frame.window;
                for (index, arg) in args.iter().enumerate() {
                    self.mov(nth(window, index), *arg);
                }
                window
            }
        };
        let import = self.em.import(item);
        Instr::CallImp {
            dst: reg(dst),
            args: reg(first),
            argc: count(args.len()),
            import,
        }
    }

    /// `std::ui::slider::<LO, HI>(props)`: the import receives `props`, then `LO` and `HI` as
    /// `Int`s, because imports carry no generic arguments. The three arguments are placed in
    /// fresh consecutive scratch registers, so `import_call` never moves one over another.
    fn slider_call(&mut self, item: ItemId, fn_ref: &FnRef, args: &[R], dst: R) -> Instr {
        let mut window: SmallVec<[R; 3]> = SmallVec::new();
        for arg in args {
            let slot = self.temp();
            self.mov(slot, *arg);
            window.push(slot);
        }
        for arg in fn_ref.args.iter().take(2) {
            let value = match arg {
                GenericArg::Const(value) => match subst_const(value, &self.env) {
                    Const::Value(value) => i64::try_from(value).unwrap_or(0),
                    _ => 0,
                },
                GenericArg::Type(_) => 0,
            };
            let bound = self.temp();
            self.emit(Instr::Int {
                dst: reg(bound),
                value,
            });
            window.push(bound);
        }
        self.import_call(item, &window, dst)
    }

    /// What `RAND` draws for a value of the type `arg`: a fieldless enum or an `Int<lo, hi>`.
    pub(super) fn rand_domain(&mut self, arg: Option<&GenericArg>) -> Option<RandDomain> {
        let Some(GenericArg::Type(ty)) = arg else {
            return None;
        };
        let ty = subst_ty(ty, &self.env);
        match ty.kind() {
            TyKind::Adt { item, .. } => Some(RandDomain::Enum(self.em.type_id(*item))),
            TyKind::Bounded { .. } => Some(RandDomain::Int(self.em.int_type(int_type_of(&ty)))),
            _ => None,
        }
    }

    /// A closure value: its captures are copied into the closure.
    pub(super) fn closure(&mut self, expr: ExprId, captures: &[Operand], dst: R) {
        let first = self.window_operands(captures);
        let func = self.em.request(Instance::Closure {
            owner: Box::new(self.root.clone()),
            expr,
        });
        self.emit(Instr::Closure {
            dst: reg(dst),
            captures: reg(first),
            count: count(captures.len()),
            func,
        });
    }
}

/// The function of a constructor used as a value: its fields are its parameters.
pub(super) fn ctor_body(em: &mut Emitter<'_>, fn_ref: &FnRef) -> Lowered {
    let instance = Instance::Ctor(fn_ref.clone());
    let (item, variant) = match fn_ref.def {
        FnDefId::StructCtor(item) => (item, 0),
        FnDefId::VariantCtor(item, variant) => (item, variant),
        FnDefId::Item(_) => {
            let mut cg = FnCodegen::bare(em, &instance, 0);
            cg.emit(Instr::Unreachable);
            return cg.finish();
        }
    };
    let (_, params) = em.instance_shape(&instance);
    let mut cg = FnCodegen::bare(em, &instance, u32::from(params));
    let dst = cg.temp();
    let ty = cg.em.type_id(item);
    cg.emit(Instr::Adt {
        dst: reg(dst),
        first: 0,
        count: params,
        ty,
        variant,
    });
    cg.emit(Instr::Ret { src: reg(dst) });
    cg.finish()
}

/// The function of a builtin used as a value: its parameters, then the builtin called on
/// them.
pub(super) fn builtin_body(em: &mut Emitter<'_>, instance: &Instance, fn_ref: &FnRef) -> Lowered {
    let sigs = em.sigs;
    let params = match fn_ref.def {
        FnDefId::Item(item) => sigs.fn_sig(item).map(|sig| (item, sig)),
        _ => None,
    };
    let Some((item, sig)) = params else {
        let mut cg = FnCodegen::bare(em, instance, 0);
        cg.emit(Instr::Unreachable);
        return cg.finish();
    };
    let params = sig.params.len();
    let first = u32::try_from(params).unwrap_or(u32::MAX);
    let mut cg = FnCodegen::bare(em, instance, first);
    let ret = subst_ty(&sig.ret, &cg.env);
    let args: Vec<R> = (0..first).collect();
    let dst = cg.temp();
    match cg.em.std.builtin(item) {
        Some(builtin) => cg.builtin(item, builtin, fn_ref, &args, dst, &ret, None),
        None => {
            let instr = if item == cg.em.std.ui_slider {
                cg.slider_call(item, fn_ref, &args, dst)
            } else {
                cg.import_call(item, &args, dst)
            };
            cg.emit(instr);
        }
    }
    cg.emit(Instr::Ret { src: reg(dst) });
    cg.finish()
}

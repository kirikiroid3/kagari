//! Places: reads and writes of locals, story variables and their projections.
//!
//! A place is walked from its base with a cursor that is either the value itself in a
//! register, or a register holding a reference to it. Value operands of the VM follow
//! references, so a read projects with FIELD/INDEX in both cases; a write projects a
//! reference with REFFIELD/REFINDEX and ends with STORE. References to references collapse
//! in the VM: dereferencing a reference cursor keeps it.

use kagari_bytecode::Instr;

use crate::eval::machine::value::{GenericEnv, subst_ty};
use crate::mir::{Local, Place, PlaceBase, Proj};
use crate::signatures::ItemSig;
use crate::ty::{GenericArg, Ty, TyKind};

use super::regalloc::Loc;
use super::{FnCodegen, R, reg};

/// A place being walked.
#[derive(Clone, Copy)]
enum Cursor {
    /// The register holds the value.
    Val(R),
    /// The register holds a reference to the place.
    Ref(R),
}

impl Cursor {
    fn reg(self) -> R {
        match self {
            Cursor::Val(r) | Cursor::Ref(r) => r,
        }
    }
}

impl FnCodegen<'_, '_> {
    pub(super) fn loc(&self, local: Local) -> Loc {
        self.frame.loc(local)
    }

    /// The register holding the value of the index local `local`.
    fn index_local(&mut self, local: Local) -> R {
        match self.loc(local) {
            Loc::Reg(r) => r,
            Loc::Cell(cell) => {
                let t = self.temp();
                self.emit(Instr::Load {
                    dst: reg(t),
                    src: reg(cell),
                });
                t
            }
            Loc::Dead => self.temp(),
        }
    }

    fn base(&mut self, base: PlaceBase) -> Cursor {
        match base {
            PlaceBase::Local(local) => match self.loc(local) {
                Loc::Reg(r) => Cursor::Val(r),
                Loc::Cell(r) => Cursor::Ref(r),
                Loc::Dead => Cursor::Val(self.temp()),
            },
            PlaceBase::StoryVar(item) => {
                let t = self.temp();
                match self.em.statics.get(&item).copied() {
                    Some(var) => self.emit(Instr::RefStatic { dst: reg(t), var }),
                    None => self.internal_error("a story variable has no static"),
                }
                Cursor::Ref(t)
            }
        }
    }

    /// Applies `proj` to `cur`. A read projects values; a write projects references.
    /// `dst` receives the result of a field or index projection.
    fn project(&mut self, cur: Cursor, proj: Proj, read: bool, dst: Option<R>) -> Cursor {
        match proj {
            Proj::Downcast(_) => cur,
            Proj::Deref => Cursor::Ref(cur.reg()),
            Proj::Field(index) => {
                let d = dst.unwrap_or_else(|| self.temp());
                let index = u16::try_from(index).unwrap_or(u16::MAX);
                match cur {
                    Cursor::Ref(base) if !read => {
                        self.emit(Instr::RefField {
                            dst: reg(d),
                            base: reg(base),
                            index,
                        });
                        Cursor::Ref(d)
                    }
                    _ => {
                        self.emit(Instr::Field {
                            dst: reg(d),
                            src: reg(cur.reg()),
                            index,
                        });
                        Cursor::Val(d)
                    }
                }
            }
            Proj::Index(local) => {
                let index = self.index_local(local);
                let d = dst.unwrap_or_else(|| self.temp());
                match cur {
                    Cursor::Ref(base) if !read => {
                        self.emit(Instr::RefIndex {
                            dst: reg(d),
                            base: reg(base),
                            index: reg(index),
                        });
                        Cursor::Ref(d)
                    }
                    _ => {
                        self.emit(Instr::Index {
                            dst: reg(d),
                            base: reg(cur.reg()),
                            index: reg(index),
                        });
                        Cursor::Val(d)
                    }
                }
            }
        }
    }

    fn walk(&mut self, place: &Place, read: bool) -> Cursor {
        let mut cur = self.base(place.base);
        for proj in &place.proj {
            cur = self.project(cur, *proj, read, None);
        }
        cur
    }

    /// A register holding the value of `place` or a reference to it, for an operand the VM
    /// dereferences itself.
    pub(super) fn read_auto(&mut self, place: &Place) -> R {
        self.walk(place, true).reg()
    }

    /// Reads the value of `place` into `dst`.
    pub(super) fn read_into(&mut self, place: &Place, dst: R) {
        // The last projection that computes something writes `dst` directly.
        let last = place
            .proj
            .iter()
            .rposition(|proj| !matches!(proj, Proj::Downcast(_)));
        if let Some(last) = last
            && let Some(proj @ (Proj::Field(_) | Proj::Index(_))) = place.proj.get(last).copied()
        {
            let mut cur = self.base(place.base);
            for proj in place.proj.iter().take(last) {
                cur = self.project(cur, *proj, true, None);
            }
            self.project(cur, proj, true, Some(dst));
            return;
        }
        match self.walk(place, true) {
            Cursor::Val(r) => self.mov(dst, r),
            // A reference to a place holding a reference stands for that reference.
            Cursor::Ref(r) if self.place_ty(place).is_some_and(|ty| is_ref(&ty)) => {
                self.mov(dst, r);
            }
            Cursor::Ref(r) => self.emit(Instr::Load {
                dst: reg(dst),
                src: reg(r),
            }),
        }
    }

    /// A register holding the value of `place`: the local's own register when it is a
    /// bare one.
    pub(super) fn read_raw(&mut self, place: &Place) -> R {
        if let Some(local) = place.as_local()
            && let Loc::Reg(r) = self.loc(local)
        {
            return r;
        }
        let t = self.temp();
        self.read_into(place, t);
        t
    }

    /// Makes `dst` a reference to `place`.
    pub(super) fn reference(&mut self, place: &Place, dst: R) {
        match self.walk(place, false) {
            Cursor::Ref(r) => self.mov(dst, r),
            // Not a celled place (the MIR always cells a borrowed local): a reference to
            // a copy.
            Cursor::Val(r) => self.emit(Instr::Cell {
                dst: reg(dst),
                init: reg(r),
            }),
        }
    }

    /// Writes the value in `value` to `place`.
    pub(super) fn write(&mut self, place: &Place, value: R) {
        if let Some(local) = place.as_local()
            && let Loc::Reg(r) = self.loc(local)
        {
            self.mov(r, value);
            return;
        }
        if let Some(local) = place.as_local()
            && self.loc(local) == Loc::Dead
        {
            return;
        }
        match self.walk(place, false) {
            Cursor::Ref(r) => self.emit(Instr::Store {
                dst: reg(r),
                value: reg(value),
            }),
            Cursor::Val(_) => {
                self.internal_error("a place written through a projection has no cell")
            }
        }
    }

    /// The register a value for `place` is computed into: the local's own register when
    /// it is a bare one, else a scratch register for [`Self::finish_dest`].
    pub(super) fn dest(&mut self, place: &Place) -> R {
        if let Some(local) = place.as_local()
            && let Loc::Reg(r) = self.loc(local)
        {
            return r;
        }
        self.temp()
    }

    /// Completes a write to `place` whose value was computed into `value = dest(place)`.
    pub(super) fn finish_dest(&mut self, place: &Place, value: R) {
        if let Some(local) = place.as_local()
            && let Loc::Reg(_) = self.loc(local)
        {
            return;
        }
        self.write(place, value);
    }

    /// The type of `place`, substituted; `None` when it cannot be followed.
    pub(super) fn place_ty(&self, place: &Place) -> Option<Ty> {
        let body = self.body?;
        let sigs = self.em.sigs;
        let mut ty = match place.base {
            PlaceBase::Local(local) => body.local_decl(local)?.ty.clone(),
            PlaceBase::StoryVar(item) => match sigs.get(item)? {
                ItemSig::Static { ty, .. } => ty.clone(),
                _ => return None,
            },
        };
        let mut variant = 0u32;
        for proj in &place.proj {
            let ty_now = subst_ty(&ty, &self.env);
            ty = match (proj, ty_now.kind()) {
                (Proj::Downcast(index), _) => {
                    variant = *index;
                    continue;
                }
                (Proj::Deref, TyKind::Ref { inner, .. }) => inner.clone(),
                (Proj::Field(index), TyKind::Tuple(fields)) => fields.get(*index as usize)?.clone(),
                (Proj::Field(index), TyKind::Adt { item, args }) => {
                    let field = sigs
                        .adt(*item)?
                        .variants
                        .get(variant as usize)?
                        .fields
                        .get(*index as usize)?;
                    let env = GenericEnv::for_item(sigs, *item, args.clone());
                    subst_ty(&field.ty, &env)
                }
                (Proj::Index(_), TyKind::Array { elem, .. }) => elem.clone(),
                (Proj::Index(_), TyKind::Adt { args, .. }) => match args.first()? {
                    GenericArg::Type(elem) => elem.clone(),
                    GenericArg::Const(_) => return None,
                },
                _ => return None,
            };
            variant = 0;
        }
        Some(subst_ty(&ty, &self.env))
    }
}

pub(super) fn is_ref(ty: &Ty) -> bool {
    matches!(ty.kind(), TyKind::Ref { .. })
}

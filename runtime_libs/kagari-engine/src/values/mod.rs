//! Std values: the std types resolved against the program, construction and the primitives
//! the typed decoders build on.

mod decode;
mod table;
#[cfg(test)]
mod tests;

use std::rc::Rc;

pub(crate) use decode::*;
pub(crate) use table::{STD_ADTS, StdTy};

use kagari_vm::{Adt, AdtTy, RefRoot, Trap, TrapKind, TypeTable, Value, Vm};

/// The `AdtTy` of every std type in [`STD_ADTS`].
pub(crate) struct StdTypes {
    tys: [AdtTy; StdTy::COUNT],
}

impl StdTypes {
    /// Resolves every std type: the module's type when the program interned it, else a
    /// synthetic one.
    pub fn resolve(types: &mut TypeTable) -> StdTypes {
        let tys = std::array::from_fn(|index| {
            let decl = &STD_ADTS[index];
            let names: Vec<&str> = decl.variants.iter().map(|v| v.name).collect();
            types.resolve(decl.path, &names)
        });
        StdTypes { tys }
    }

    pub fn ty(&self, t: StdTy) -> AdtTy {
        self.tys[t as usize]
    }

    pub fn make(&self, t: StdTy, variant: u32, fields: Vec<Value>) -> Value {
        Value::Adt(Rc::new(Adt {
            ty: self.ty(t),
            variant,
            fields,
        }))
    }

    /// A variant without fields.
    pub fn unit(&self, t: StdTy, variant: u32) -> Value {
        self.make(t, variant, Vec::new())
    }

    /// `(variant, fields)` after checking the value is a `t`; derefs nothing.
    pub fn variant<'v>(&self, v: &'v Value, t: StdTy) -> Result<(u32, &'v [Value]), Trap> {
        match v {
            Value::Adt(adt) if adt.ty == self.ty(t) => Ok((adt.variant, &adt.fields)),
            _ => Err(Trap::new(TrapKind::TagMismatch {
                expected: STD_ADTS[t as usize].path,
                found: tag_name(v),
            })),
        }
    }

    /// The fields of struct `t`.
    pub fn fields<'v>(&self, v: &'v Value, t: StdTy) -> Result<&'v [Value], Trap> {
        Ok(self.variant(v, t)?.1)
    }

    pub fn none(&self) -> Value {
        self.unit(StdTy::Option, 0)
    }

    pub fn some(&self, v: Value) -> Value {
        self.make(StdTy::Option, 1, vec![v])
    }

    pub fn ok(&self, v: Value) -> Value {
        self.make(StdTy::Result, 0, vec![v])
    }

    pub fn err(&self, v: Value) -> Value {
        self.make(StdTy::Result, 1, vec![v])
    }

    pub fn option<'v>(&self, v: &'v Value) -> Result<Option<&'v Value>, Trap> {
        match self.variant(v, StdTy::Option)? {
            (1, [inner]) => Ok(Some(inner)),
            (0, []) => Ok(None),
            _ => Err(host_trap("malformed `std::Option`")),
        }
    }
}

/// A trap raised by an import.
pub(crate) fn host_trap(msg: impl Into<String>) -> Trap {
    Trap::new(TrapKind::Host(msg.into()))
}

/// The tag name traps report for `v`.
pub(crate) fn tag_name(v: &Value) -> &'static str {
    v.tag()
}

/// By-value argument `i`; a cell-rooted `Value::Ref` is loaded through `vm.load`.
pub(crate) fn arg(vm: &Vm, path: &str, args: &[Value], i: usize) -> Result<Value, Trap> {
    let Some(value) = args.get(i) else {
        return Err(host_trap(format!(
            "`{path}` expects {} arguments, found {}",
            i.saturating_add(1),
            args.len()
        )));
    };
    match value {
        Value::Ref(r) if matches!(r.root, RefRoot::Cell(_)) => vm.load(r),
        other => Ok(other.clone()),
    }
}

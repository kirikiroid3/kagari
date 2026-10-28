//! References: resolution through chains of references, reads, and copy-on-write writes.
//!
//! References to references collapse: a reference whose path meets a stored reference (or
//! whose target is one) is equivalent to that reference extended by the rest of the path.

use std::cell::RefCell;
use std::rc::Rc;

use crate::trap::{Trap, TrapKind, mismatch};
use crate::value::{Proj, Ref, RefRoot, Value};

/// A reference cycle can only come from a cell that stores a reference to itself.
const MAX_REF_HOPS: u32 = 65_536;

type Cell = Rc<RefCell<Value>>;
type Path = Rc<Vec<Proj>>;

fn borrowed() -> Trap {
    Trap::host("cell already borrowed")
}

fn root_cell(root: &RefRoot) -> Result<Cell, Trap> {
    match root {
        RefRoot::Cell(cell) => Ok(cell.clone()),
        RefRoot::Host(_) => Err(Trap::new(TrapKind::HostPartAccess)),
    }
}

/// `value`'s projection `proj`, by reference.
pub(crate) fn project(value: &Value, proj: Proj) -> Result<&Value, Trap> {
    let out = match (value, proj) {
        (Value::Tuple(items), Proj::Field(i)) => items.get(usize::from(i)),
        (Value::Adt(adt), Proj::Field(i)) => adt.fields.get(usize::from(i)),
        (Value::Array(items) | Value::Vec(items), Proj::Index(i)) => {
            usize::try_from(i).ok().and_then(|i| items.get(i))
        }
        (other, Proj::Field(_)) => return Err(mismatch("Tuple or Adt", other.tag())),
        (other, Proj::Index(_)) => return Err(mismatch("Array or Vec", other.tag())),
    };
    out.ok_or(Trap::new(TrapKind::IndexOutOfRange))
}

/// `value`'s projection `proj`, mutably; aggregates are unshared first (copy on write).
fn project_mut(value: &mut Value, proj: Proj) -> Result<&mut Value, Trap> {
    let out = match (value, proj) {
        (Value::Tuple(items), Proj::Field(i)) => Rc::make_mut(items).get_mut(usize::from(i)),
        (Value::Adt(adt), Proj::Field(i)) => Rc::make_mut(adt).fields.get_mut(usize::from(i)),
        (Value::Array(items) | Value::Vec(items), Proj::Index(i)) => match usize::try_from(i) {
            Ok(i) => Rc::make_mut(items).get_mut(i),
            Err(_) => None,
        },
        (other, Proj::Field(_)) => return Err(mismatch("Tuple or Adt", other.tag())),
        (other, Proj::Index(_)) => return Err(mismatch("Array or Vec", other.tag())),
    };
    out.ok_or(Trap::new(TrapKind::IndexOutOfRange))
}

/// One walk of `path` from `cell`: `Some` of the reference to restart from when a stored
/// reference lies along the path or at its end, `None` when the walk reached a plain target.
fn walk(cell: &Cell, path: &Path) -> Result<Option<(Cell, Path)>, Trap> {
    let guard = cell.try_borrow().map_err(|_| borrowed())?;
    let mut current: &Value = &guard;
    let mut k = 0;
    loop {
        if let Value::Ref(inner) = current {
            let root = root_cell(&inner.root)?;
            let rest = path.get(k..).unwrap_or_default();
            let path = if rest.is_empty() {
                inner.path.clone()
            } else {
                Rc::new(inner.path.iter().chain(rest).copied().collect())
            };
            return Ok(Some((root, path)));
        }
        let Some(&proj) = path.get(k) else {
            return Ok(None);
        };
        current = project(current, proj)?;
        k = k.saturating_add(1);
    }
}

/// An equivalent reference with no reference along its path and none at its target.
pub(crate) fn resolve(r: &Ref) -> Result<(Cell, Path), Trap> {
    let mut cell = root_cell(&r.root)?;
    let mut path = r.path.clone();
    let mut hops: u32 = 0;
    while let Some((next_cell, next_path)) = walk(&cell, &path)? {
        hops = hops.saturating_add(1);
        if hops > MAX_REF_HOPS {
            return Err(Trap::new(TrapKind::FuelExhausted));
        }
        cell = next_cell;
        path = next_path;
    }
    Ok((cell, path))
}

/// Runs `f` on the target of a resolved reference.
pub(crate) fn with_target<R>(
    cell: &Cell,
    path: &[Proj],
    f: impl FnOnce(&Value) -> Result<R, Trap>,
) -> Result<R, Trap> {
    let guard = cell.try_borrow().map_err(|_| borrowed())?;
    let mut current: &Value = &guard;
    for &proj in path {
        current = project(current, proj)?;
    }
    f(current)
}

/// The value a reference points to; `Uninit` panics.
pub(crate) fn read(r: &Ref) -> Result<Value, Trap> {
    let (cell, path) = resolve(r)?;
    with_target(&cell, &path, |v| match v {
        Value::Uninit => Err(Trap::new(TrapKind::ReadUninit)),
        v => Ok(v.clone()),
    })
}

/// Runs `f` on the slot a reference points to, unsharing every aggregate along the path.
pub(crate) fn with_mut<R>(r: &Ref, f: impl FnOnce(&mut Value) -> R) -> Result<R, Trap> {
    let (cell, path) = resolve(r)?;
    let mut guard = cell.try_borrow_mut().map_err(|_| borrowed())?;
    let mut slot: &mut Value = &mut guard;
    for &proj in path.iter() {
        slot = project_mut(slot, proj)?;
    }
    Ok(f(slot))
}

/// Writes `value` through a reference.
pub(crate) fn write(r: &Ref, value: Value) -> Result<(), Trap> {
    with_mut(r, |slot| *slot = value)
}

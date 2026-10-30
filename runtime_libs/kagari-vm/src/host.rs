//! The engine side: host objects carried in values, host functions handed to bytecode, and the
//! [`Host`] trait the VM calls for imports.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

use kagari_bytecode::ImportId;

use crate::remap::{Remap, RemapError};
use crate::trap::Trap;
use crate::value::Value;
use crate::vm::Vm;

/// An engine object with value semantics. `Value::Host` copies share the `Rc`; a host import
/// that mutates goes through [`HostObj::make_mut`], which clones when shared (copy on write).
#[derive(Clone)]
pub struct HostObj(pub Rc<dyn HostObject>);

/// The behaviour every engine object provides.
pub trait HostObject: Any {
    /// A deep copy, for copy on write.
    fn clone_object(&self) -> Rc<dyn HostObject>;
    /// The name traps report.
    fn type_name(&self) -> &'static str;
    /// Structural equality for EQ/NE; `None` means "not comparable", which panics
    /// (`TagMismatch`).
    fn eq_object(&self, other: &dyn HostObject) -> Option<bool>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// The object rebuilt for another program (scene hot swap): its asset, type and static
    /// indices by path. `Ok(None)` when it cannot move, which is the default.
    fn remap(&self, _remap: &mut Remap<'_>) -> Result<Option<HostObj>, RemapError> {
        Ok(None)
    }
}

impl HostObj {
    pub fn new<T: HostObject>(obj: T) -> HostObj {
        HostObj(Rc::new(obj))
    }

    pub fn downcast<T: 'static>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref::<T>()
    }

    /// The object as `T`, unshared first: a shared object is cloned with
    /// [`HostObject::clone_object`]. `None`, without cloning, when it is not a `T`.
    pub fn make_mut<T: HostObject + Clone>(this: &mut HostObj) -> Option<&mut T> {
        this.downcast::<T>()?;
        if Rc::get_mut(&mut this.0).is_none() {
            this.0 = this.0.clone_object();
        }
        Rc::get_mut(&mut this.0)?.as_any_mut().downcast_mut::<T>()
    }
}

impl fmt::Debug for HostObj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HostObj({})", self.0.type_name())
    }
}

/// A callable the engine hands to bytecode (a signal getter or setter). CALLVAL with kind `Fn`
/// accepts it and dispatches to [`Host::call_host_fn`]. EQ on it panics, like closures.
#[derive(Clone, Debug)]
pub struct HostFn {
    pub id: u64,
    pub params: u16,
}

/// The engine: imports and host functions.
pub trait Host {
    fn import(&mut self, vm: &mut Vm, import: ImportId, args: Vec<Value>) -> Result<Value, Trap>;
    fn call_host_fn(&mut self, vm: &mut Vm, f: &HostFn, args: Vec<Value>) -> Result<Value, Trap>;
}

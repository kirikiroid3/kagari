//! The `&mut` parts of a screen that imports hand to bytecode (`&mut Entity`, `&mut Prop<T>`,
//! pending changes, …), and the resolution of an import's receiver.

use std::any::Any;
use std::rc::Rc;

use kagari_vm::{HostObj, HostObject, Ref, RefRoot, Trap, Value, Vm};

use super::model::{PartTarget, PendingChange, ScreenObj};
use crate::values::host_trap;

/// A part of the screen behind `screen`.
#[derive(Clone, Debug)]
pub(crate) struct ScreenPart {
    pub screen: Ref,
    pub target: PartTarget,
}

/// A pending change of the screen behind `screen` (`Change`, `Swap`, `ChannelChange`).
#[derive(Clone, Debug)]
pub(crate) struct PendingObj {
    pub screen: Ref,
    pub change: PendingChange,
}

macro_rules! part_object {
    ($ty:ty, $name:literal) => {
        impl HostObject for $ty {
            fn clone_object(&self) -> Rc<dyn HostObject> {
                Rc::new(self.clone())
            }

            fn type_name(&self) -> &'static str {
                $name
            }

            /// Parts are never values: comparing them panics.
            fn eq_object(&self, _other: &dyn HostObject) -> Option<bool> {
                None
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
        }
    };
}

part_object!(ScreenPart, "ScreenPart");
part_object!(PendingObj, "Change");

/// What an import's receiver denotes.
pub(crate) enum Receiver {
    /// A reference to the cell (or field) holding a `Screen`.
    Screen(Ref),
    /// A `Screen` passed by value (a `&self` read).
    ScreenValue(HostObj),
    Part(ScreenPart),
    Pending(PendingObj),
}

/// The receiver `v` of the import `path`.
pub(crate) fn receiver(vm: &Vm, path: &str, v: Option<&Value>) -> Result<Receiver, Trap> {
    let bad = || bad_receiver(path);
    match v {
        Some(Value::Ref(r)) => match &r.root {
            RefRoot::Host(obj) => {
                if let Some(part) = obj.downcast::<ScreenPart>() {
                    Ok(Receiver::Part(part.clone()))
                } else if let Some(pending) = obj.downcast::<PendingObj>() {
                    Ok(Receiver::Pending(pending.clone()))
                } else {
                    Err(bad())
                }
            }
            RefRoot::Cell(_) => match vm.with_mut(r, |value| value.clone())? {
                Value::Host(obj) if obj.downcast::<ScreenObj>().is_some() => {
                    Ok(Receiver::Screen(r.clone()))
                }
                _ => Err(bad()),
            },
        },
        Some(Value::Host(obj)) if obj.downcast::<ScreenObj>().is_some() => {
            Ok(Receiver::ScreenValue(obj.clone()))
        }
        _ => Err(bad()),
    }
}

/// The trap of an import whose receiver is not the type its path names.
pub(crate) fn bad_receiver(path: &str) -> Trap {
    host_trap(format!("`{path}`: receiver is not a `{}`", expected(path)))
}

/// The receiver type an import path names: its second-to-last segment.
fn expected(path: &str) -> &str {
    let mut segments = path.rsplit("::");
    segments.next();
    segments.next().unwrap_or(path)
}

/// Runs `f` on the screen behind `screen`, unshared first (copy on write).
pub(crate) fn with_screen<R>(
    vm: &Vm,
    screen: &Ref,
    f: impl FnOnce(&mut ScreenObj) -> Result<R, Trap>,
) -> Result<R, Trap> {
    vm.with_mut(screen, |value| match value {
        Value::Host(obj) => match HostObj::make_mut::<ScreenObj>(obj) {
            Some(screen) => f(screen),
            None => Err(host_trap("the reference does not hold a `Screen`")),
        },
        _ => Err(host_trap("the reference does not hold a `Screen`")),
    })?
}

fn host_ref(obj: HostObj) -> Value {
    Value::Ref(Ref {
        root: RefRoot::Host(obj),
        path: Rc::new(Vec::new()),
    })
}

/// A `&mut` to a part of the screen behind `screen`.
pub(crate) fn part_ref(screen: &Ref, target: PartTarget) -> Value {
    host_ref(HostObj::new(ScreenPart {
        screen: screen.clone(),
        target,
    }))
}

/// A `&mut` to a pending change of the screen behind `screen`.
pub(crate) fn pending_ref(screen: &Ref, change: PendingChange) -> Value {
    host_ref(HostObj::new(PendingObj {
        screen: screen.clone(),
        change,
    }))
}

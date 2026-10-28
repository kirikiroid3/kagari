#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::any::Any;
use std::rc::Rc;

use crate::host::{HostObj, HostObject};
use crate::rng::Rng;

#[test]
fn prng_vectors() {
    let mut rng = Rng::from_seed(0);
    assert_eq!(
        (rng.s0, rng.s1),
        (0xE220_A839_7B1D_CDAF, 0x6E78_9E6A_A1B9_65F4)
    );
    assert_eq!(rng.next_u64(), 0x6F68_E1E7_E264_6EE1);
}

#[derive(Clone)]
struct Counter(u32);

impl HostObject for Counter {
    fn clone_object(&self) -> Rc<dyn HostObject> {
        Rc::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "Counter"
    }
    fn eq_object(&self, other: &dyn HostObject) -> Option<bool> {
        other
            .as_any()
            .downcast_ref::<Counter>()
            .map(|other| other.0 == self.0)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Another host type, for the failed downcast (`String` is not a `HostObject`).
#[derive(Clone)]
struct Other;

impl HostObject for Other {
    fn clone_object(&self) -> Rc<dyn HostObject> {
        Rc::new(self.clone())
    }
    fn type_name(&self) -> &'static str {
        "Other"
    }
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

#[test]
fn host_objects_copy_on_write() {
    let a = HostObj::new(Counter(1));
    let mut b = a.clone();
    HostObj::make_mut::<Counter>(&mut b).unwrap().0 = 2;
    assert_eq!(a.downcast::<Counter>().unwrap().0, 1);
    assert_eq!(b.downcast::<Counter>().unwrap().0, 2);

    let mut unique = HostObj::new(Counter(5));
    let before = Rc::as_ptr(&unique.0).cast::<()>();
    HostObj::make_mut::<Counter>(&mut unique).unwrap().0 = 6;
    assert!(std::ptr::eq(before, Rc::as_ptr(&unique.0).cast::<()>()));
    assert_eq!(unique.downcast::<Counter>().unwrap().0, 6);

    let shared = unique.clone();
    assert!(HostObj::make_mut::<Other>(&mut unique).is_none());
    // A failed downcast does not clone.
    assert!(Rc::ptr_eq(&shared.0, &unique.0));
}

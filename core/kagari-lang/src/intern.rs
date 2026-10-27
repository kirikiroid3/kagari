//! A hash-consing interner. Interned values are shared behind one `triomphe::Arc` per distinct
//! value, so equality is a pointer comparison and hashing writes the cached content hash.
//! Tables are never garbage-collected.

use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Deref;
use std::sync::{Mutex, PoisonError};

use rustc_hash::{FxBuildHasher, FxHashMap};

const SHARDS: usize = 16;

/// A type with a global intern table; implement with [`internable!`].
pub trait Internable: Clone + Eq + Hash + Send + Sync + 'static {
    fn table() -> &'static InternTable<Self>;
}

/// The global table of one interned type, sharded by content hash.
pub struct InternTable<T> {
    shards: [Mutex<FxHashMap<T, Interned<T>>>; SHARDS],
}

impl<T> Default for InternTable<T> {
    fn default() -> Self {
        Self {
            shards: std::array::from_fn(|_| Mutex::new(FxHashMap::default())),
        }
    }
}

struct Node<T> {
    hash: u64,
    value: T,
}

/// A shared, deduplicated value.
pub struct Interned<T>(triomphe::Arc<Node<T>>);

impl<T: Internable> Interned<T> {
    pub fn new(value: T) -> Self {
        let hash = FxBuildHasher.hash_one(&value);
        // The remainder is below `SHARDS`, so the index is in bounds.
        let shard = &T::table().shards[hash.checked_rem(SHARDS as u64).unwrap_or(0) as usize];
        let mut map = shard.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = map.get(&value) {
            return found.clone();
        }
        let interned = Self(triomphe::Arc::new(Node {
            hash,
            value: value.clone(),
        }));
        map.insert(value, interned.clone());
        interned
    }
}

impl<T> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> PartialEq for Interned<T> {
    fn eq(&self, other: &Self) -> bool {
        triomphe::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Eq for Interned<T> {}

impl<T> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.hash);
    }
}

impl<T> Deref for Interned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0.value
    }
}

impl<T: fmt::Debug> fmt::Debug for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.value.fmt(f)
    }
}

/// Implements [`Internable`] for a type with a function-local static table.
macro_rules! internable {
    ($t:ty) => {
        impl $crate::intern::Internable for $t {
            fn table() -> &'static $crate::intern::InternTable<Self> {
                static TABLE: std::sync::LazyLock<$crate::intern::InternTable<$t>> =
                    std::sync::LazyLock::new(Default::default);
                &TABLE
            }
        }
    };
}
pub(crate) use internable;

//! `.kisave`: a JSONB-like save database in 16 KiB pages, edited in place in memory and
//! written back whole.
//!
//! Layout (V000, little-endian, unaligned): the magic, a header `root: u64, end: u64,
//! reserved: [u64; 2]`, then a bump-allocated heap of table heads, array heads, field names
//! and out-of-line values. Edits never free space; [`Save::compact`] copies the live data
//! into a fresh save.

mod compact;
mod error;
mod layout;
mod read;
mod validate;
mod write;

#[cfg(test)]
mod tests;

use std::fmt;

pub use error::Error;
pub use layout::{DEFAULT_CAP, MAGIC, MAX_DEPTH, PAGE_SIZE, ROOT_CAP, ValueType};
pub use read::{ArrayRef, TableRef, ValueRef};
pub use write::{ArrayMut, Scalar, TableMut};

use layout::{END_PTR, HEADER_END, INITIAL_END, ROOT_PTR};

/// A save database. Every `Save` is well-formed: [`Save::from_bytes`] validates eagerly and
/// every edit preserves the layout rules.
#[derive(Clone, PartialEq, Eq)]
pub struct Save {
    bytes: Vec<u8>,
}

impl Save {
    /// An empty save: one page holding the header and an empty root table.
    pub fn new() -> Save {
        let mut bytes = vec![0; PAGE_SIZE as usize];
        bytes[..16].copy_from_slice(&MAGIC);
        bytes[16..24].copy_from_slice(&HEADER_END.to_le_bytes());
        bytes[24..32].copy_from_slice(&INITIAL_END.to_le_bytes());
        bytes[56..64].copy_from_slice(&ROOT_CAP.to_le_bytes());
        Save { bytes }
    }

    /// Validates `bytes` in full and wraps them.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Save, Error> {
        validate::validate(&bytes)?;
        Ok(Save { bytes })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The bump pointer: the first byte never allocated.
    pub fn used_bytes(&self) -> u64 {
        self.u64_at(END_PTR)
    }

    pub fn root(&self) -> TableRef<'_> {
        TableRef::new(self, self.u64_at(ROOT_PTR))
    }

    pub fn root_mut(&mut self) -> TableMut<'_> {
        let off = self.u64_at(ROOT_PTR);
        TableMut::new(self, off, ROOT_PTR, 0)
    }
}

impl Default for Save {
    fn default() -> Save {
        Save::new()
    }
}

impl fmt::Debug for Save {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Save")
            .field("len", &self.bytes.len())
            .field("used_bytes", &self.used_bytes())
            .finish()
    }
}

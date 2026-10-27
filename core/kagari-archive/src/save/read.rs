//! Borrowed read views over a [`Save`].

use std::fmt;

use super::Save;
use super::layout::{CELL_VALUE, ELEM_LEN, FIELD_CELL, FIELD_LEN, ValueType, item};
use crate::le;

/// A value read from a save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueRef<'a> {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    I128(i128),
    U128(u128),
    /// `mantissa / 10^scale`.
    Decimal {
        mantissa: i128,
        scale: u32,
    },
    Rat {
        num: i128,
        den: i128,
    },
    String(&'a str),
    Bytes(&'a [u8]),
    Table(TableRef<'a>),
    Array(ArrayRef<'a>),
}

impl Save {
    /// The `u64` at `off`. A `Save` is well-formed, so the read is in bounds.
    pub(crate) fn u64_at(&self, off: u64) -> u64 {
        le::u64_at(&self.bytes, off).unwrap_or(0)
    }

    /// The `(type id, value)` of the cell at `cell`.
    pub(crate) fn cell(&self, cell: u64) -> (u16, u64) {
        (
            le::u16_at(&self.bytes, cell).unwrap_or(0),
            self.u64_at(cell.saturating_add(CELL_VALUE)),
        )
    }

    /// The bytes a String or Bytes head at `head` points to.
    pub(crate) fn data(&self, head: u64) -> Option<&[u8]> {
        le::slice_at(
            &self.bytes,
            self.u64_at(head),
            self.u64_at(head.saturating_add(8)),
        )
    }

    pub(crate) fn value_at(&self, cell: u64) -> Option<ValueRef<'_>> {
        let b = &self.bytes;
        let (id, v) = self.cell(cell);
        Some(match ValueType::from_id(id)? {
            ValueType::Null => ValueRef::Null,
            ValueType::Bool => ValueRef::Bool(v != 0),
            ValueType::I64 => ValueRef::I64(v.cast_signed()),
            ValueType::U64 => ValueRef::U64(v),
            ValueType::I128 => ValueRef::I128(le::i128_at(b, v)?),
            ValueType::U128 => ValueRef::U128(le::u128_at(b, v)?),
            ValueType::Decimal => ValueRef::Decimal {
                mantissa: le::i128_at(b, v)?,
                scale: le::u32_at(b, v.checked_add(16)?)?,
            },
            ValueType::Rat => ValueRef::Rat {
                num: le::i128_at(b, v)?,
                den: le::i128_at(b, v.checked_add(16)?)?,
            },
            ValueType::String => ValueRef::String(std::str::from_utf8(self.data(v)?).ok()?),
            ValueType::Bytes => ValueRef::Bytes(self.data(v)?),
            ValueType::Table => ValueRef::Table(TableRef::new(self, v)),
            ValueType::Array => ValueRef::Array(ArrayRef { save: self, off: v }),
        })
    }
}

/// A table: fields sorted by name.
#[derive(Clone, Copy)]
pub struct TableRef<'a> {
    save: &'a Save,
    off: u64,
}

impl<'a> TableRef<'a> {
    pub(crate) fn new(save: &'a Save, off: u64) -> TableRef<'a> {
        TableRef { save, off }
    }

    pub fn len(&self) -> u64 {
        self.save.u64_at(self.off)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The slot capacity; inserts past it relocate the table.
    pub fn cap(&self) -> u64 {
        self.save.u64_at(self.off.saturating_add(8))
    }

    fn slot(&self, i: u64) -> Option<u64> {
        item(self.off, i, FIELD_LEN)
    }

    fn name_bytes(&self, i: u64) -> Option<&'a [u8]> {
        let slot = self.slot(i)?;
        le::slice_at(
            &self.save.bytes,
            self.save.u64_at(slot),
            self.save.u64_at(slot.checked_add(8)?),
        )
    }

    fn value(&self, i: u64) -> Option<ValueRef<'a>> {
        self.save.value_at(self.slot(i)?.checked_add(FIELD_CELL)?)
    }

    /// `Ok(index)` of `name`, or `Err(index)` where it would be inserted.
    pub(crate) fn find(&self, name: &str) -> Result<u64, u64> {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo.midpoint(hi);
            match self
                .name_bytes(mid)
                .unwrap_or_default()
                .cmp(name.as_bytes())
            {
                std::cmp::Ordering::Less => lo = mid.saturating_add(1),
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Ok(mid),
            }
        }
        Err(lo)
    }

    pub fn get(&self, name: &str) -> Option<ValueRef<'a>> {
        self.value(self.find(name).ok()?)
    }

    /// Every field, in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&'a str, ValueRef<'a>)> + use<'a> {
        let table = *self;
        (0..self.len()).filter_map(move |i| {
            let name = std::str::from_utf8(table.name_bytes(i)?).ok()?;
            Some((name, table.value(i)?))
        })
    }
}

impl fmt::Debug for TableRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TableRef").field("off", &self.off).finish()
    }
}

/// Compares the table's position, not its contents.
impl PartialEq for TableRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.off == other.off
    }
}

impl Eq for TableRef<'_> {}

/// An array of cells.
#[derive(Clone, Copy)]
pub struct ArrayRef<'a> {
    save: &'a Save,
    off: u64,
}

impl<'a> ArrayRef<'a> {
    pub(crate) fn new(save: &'a Save, off: u64) -> ArrayRef<'a> {
        ArrayRef { save, off }
    }

    pub fn len(&self) -> u64 {
        self.save.u64_at(self.off)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The element capacity; pushes past it relocate the array.
    pub fn cap(&self) -> u64 {
        self.save.u64_at(self.off.saturating_add(8))
    }

    pub fn get(&self, i: u64) -> Option<ValueRef<'a>> {
        if i >= self.len() {
            return None;
        }
        self.save.value_at(item(self.off, i, ELEM_LEN)?)
    }

    pub fn iter(&self) -> impl Iterator<Item = ValueRef<'a>> + use<'a> {
        let array = *self;
        (0..self.len()).filter_map(move |i| array.get(i))
    }
}

impl fmt::Debug for ArrayRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArrayRef").field("off", &self.off).finish()
    }
}

/// Compares the array's position, not its contents.
impl PartialEq for ArrayRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.off == other.off
    }
}

impl Eq for ArrayRef<'_> {}

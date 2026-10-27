//! In-place edits. Space is bump-allocated and never reused: a replaced value or a relocated
//! table becomes garbage until [`Save::compact`].
//!
//! A [`TableMut`] or [`ArrayMut`] knows `owner`, the address of the 8-byte pointer that holds
//! its offset (the header's root pointer or its parent's cell), and rewrites it when the node
//! relocates to grow. Child handles reborrow the `&mut Save`, so a parent cannot move while a
//! child is alive and `owner` stays valid.

use super::layout::{
    CELL_VALUE, DEFAULT_CAP, ELEM_LEN, END_PTR, FIELD_CELL, FIELD_LEN, HEAD_LEN, MAX_DEPTH,
    PAGE_SIZE, ValueType, item,
};
use super::read::{ArrayRef, TableRef, ValueRef};
use super::{Error, Save};
use crate::le;

/// A value that is not a container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar<'a> {
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
}

pub(super) fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::OutOfBounds)
}

pub(super) fn at(off: u64, i: u64, item_len: u64) -> Result<u64, Error> {
    item(off, i, item_len).ok_or(Error::OutOfBounds)
}

fn len_u64(n: usize) -> Result<u64, Error> {
    u64::try_from(n).map_err(|_| Error::TooLarge)
}

/// The child depth below `depth`, or `TooDeep`.
pub(super) fn child_depth(depth: u32) -> Result<u32, Error> {
    depth
        .checked_add(1)
        .filter(|&d| d <= MAX_DEPTH)
        .ok_or(Error::TooDeep)
}

/// The item size of a container type.
pub(super) fn item_len(ty: ValueType) -> u64 {
    if ty == ValueType::Table {
        FIELD_LEN
    } else {
        ELEM_LEN
    }
}

impl Save {
    pub(crate) fn put_u16(&mut self, off: u64, v: u16) -> Result<(), Error> {
        le::put_u16(&mut self.bytes, off, v).ok_or(Error::OutOfBounds)
    }

    pub(crate) fn put_u64(&mut self, off: u64, v: u64) -> Result<(), Error> {
        le::put_u64(&mut self.bytes, off, v).ok_or(Error::OutOfBounds)
    }

    pub(crate) fn put_bytes(&mut self, off: u64, data: &[u8]) -> Result<(), Error> {
        le::slice_at_mut(&mut self.bytes, off, len_u64(data.len())?)
            .ok_or(Error::OutOfBounds)?
            .copy_from_slice(data);
        Ok(())
    }

    fn fill_zero(&mut self, off: u64, len: u64) -> Result<(), Error> {
        le::slice_at_mut(&mut self.bytes, off, len)
            .ok_or(Error::OutOfBounds)?
            .fill(0);
        Ok(())
    }

    /// Copies `len` bytes from `src` to `dst`; the ranges may overlap.
    fn copy(&mut self, src: u64, dst: u64, len: u64) -> Result<(), Error> {
        if le::slice_at(&self.bytes, src, len).is_none()
            || le::slice_at(&self.bytes, dst, len).is_none()
        {
            return Err(Error::OutOfBounds);
        }
        let conv = |n| usize::try_from(n).map_err(|_| Error::OutOfBounds);
        let (src, dst, len) = (conv(src)?, conv(dst)?, conv(len)?);
        self.bytes.copy_within(src..src.saturating_add(len), dst);
        Ok(())
    }

    /// Reserves `len` fresh bytes at the bump pointer, growing the file by whole pages.
    pub(crate) fn alloc(&mut self, len: u64) -> Result<u64, Error> {
        let off = self.used_bytes();
        let new_end = off.checked_add(len).ok_or(Error::TooLarge)?;
        let need = usize::try_from(new_end).map_err(|_| Error::TooLarge)?;
        if need > self.bytes.len() {
            let size = new_end
                .checked_next_multiple_of(PAGE_SIZE)
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(Error::TooLarge)?;
            self.bytes.resize(size, 0);
        }
        self.put_u64(END_PTR, new_end)?;
        Ok(off)
    }

    /// Allocates an empty table or array head with room for `cap` items of `item_len`.
    pub(crate) fn new_node(&mut self, item_len: u64, cap: u64) -> Result<u64, Error> {
        let size = cap
            .checked_mul(item_len)
            .and_then(|n| n.checked_add(HEAD_LEN))
            .ok_or(Error::TooLarge)?;
        let off = self.alloc(size)?;
        self.put_u64(off, 0)?;
        self.put_u64(add(off, 8)?, cap)?;
        Ok(off)
    }

    /// Allocates a String or Bytes head followed by `data`; returns the head.
    pub(crate) fn alloc_data(&mut self, data: &[u8]) -> Result<u64, Error> {
        let len = len_u64(data.len())?;
        let head = self.alloc(HEAD_LEN.checked_add(len).ok_or(Error::TooLarge)?)?;
        let data_off = add(head, HEAD_LEN)?;
        self.put_u64(head, data_off)?;
        self.put_u64(add(head, 8)?, len)?;
        self.put_bytes(data_off, data)?;
        Ok(head)
    }

    /// Writes a fixed-size value into the head at `head`.
    fn write_fixed(&mut self, head: u64, value: Scalar<'_>) -> Result<(), Error> {
        let b = &mut self.bytes;
        match value {
            Scalar::I128(v) => le::put_i128(b, head, v),
            Scalar::U128(v) => le::put_u128(b, head, v),
            Scalar::Decimal { mantissa, scale } => le::put_i128(b, head, mantissa)
                .and_then(|()| le::put_u32(b, head.checked_add(16)?, scale)),
            Scalar::Rat { num, den } => le::put_i128(b, head, num)
                .and_then(|()| le::put_i128(b, head.checked_add(16)?, den)),
            _ => return Err(Error::TypeMismatch),
        }
        .ok_or(Error::OutOfBounds)
    }

    /// Encodes `value` as a cell's `(type id, value)`, allocating out-of-line data.
    fn encode(&mut self, value: Scalar<'_>) -> Result<(u16, u64), Error> {
        let ty = scalar_type(value);
        let v = match value {
            Scalar::Null => 0,
            Scalar::Bool(b) => u64::from(b),
            Scalar::I64(v) => v.cast_unsigned(),
            Scalar::U64(v) => v,
            Scalar::I128(_) | Scalar::U128(_) | Scalar::Decimal { .. } | Scalar::Rat { .. } => {
                let head = self.alloc(ty.fixed_len().ok_or(Error::TypeMismatch)?)?;
                self.write_fixed(head, value)?;
                head
            }
            Scalar::String(s) => self.alloc_data(s.as_bytes())?,
            Scalar::Bytes(b) => self.alloc_data(b)?,
        };
        Ok((ty.id(), v))
    }

    fn write_cell(&mut self, cell: u64, ty: u16, v: u64) -> Result<(), Error> {
        self.put_u16(cell, ty)?;
        self.put_u64(add(cell, CELL_VALUE)?, v)
    }

    /// Stores `value` in the existing cell at `cell`. A fixed-size value replacing one of the
    /// same type is rewritten in place; anything else is encoded afresh.
    fn store(&mut self, cell: u64, value: Scalar<'_>) -> Result<(), Error> {
        let (old, head) = self.cell(cell);
        let ty = scalar_type(value);
        if ty.fixed_len().is_some() && old == ty.id() {
            return self.write_fixed(head, value);
        }
        let (ty, v) = self.encode(value)?;
        self.write_cell(cell, ty, v)
    }

    /// Moves the node at `off` to a fresh region with double the capacity and points `owner`
    /// at it.
    fn grow(&mut self, off: u64, owner: u64, item_len: u64) -> Result<u64, Error> {
        let count = self.u64_at(off);
        let cap = self.u64_at(add(off, 8)?);
        let new_cap = cap.checked_mul(2).ok_or(Error::TooLarge)?.max(DEFAULT_CAP);
        let new = self.new_node(item_len, new_cap)?;
        let live = count.checked_mul(item_len).ok_or(Error::OutOfBounds)?;
        self.copy(add(off, HEAD_LEN)?, add(new, HEAD_LEN)?, live)?;
        self.put_u64(new, count)?;
        self.put_u64(owner, new)?;
        Ok(new)
    }

    /// Appends a cell to the array at `*off`, growing it first when full.
    fn push_cell(&mut self, off: &mut u64, owner: u64, ty: u16, v: u64) -> Result<u64, Error> {
        let len = self.u64_at(*off);
        if len >= self.u64_at(add(*off, 8)?) {
            *off = self.grow(*off, owner, ELEM_LEN)?;
        }
        let cell = at(*off, len, ELEM_LEN)?;
        self.write_cell(cell, ty, v)?;
        self.put_u64(*off, add(len, 1)?)?;
        Ok(cell)
    }
}

fn scalar_type(value: Scalar<'_>) -> ValueType {
    match value {
        Scalar::Null => ValueType::Null,
        Scalar::Bool(_) => ValueType::Bool,
        Scalar::I64(_) => ValueType::I64,
        Scalar::U64(_) => ValueType::U64,
        Scalar::I128(_) => ValueType::I128,
        Scalar::U128(_) => ValueType::U128,
        Scalar::Decimal { .. } => ValueType::Decimal,
        Scalar::Rat { .. } => ValueType::Rat,
        Scalar::String(_) => ValueType::String,
        Scalar::Bytes(_) => ValueType::Bytes,
    }
}

/// A mutable table handle.
pub struct TableMut<'a> {
    save: &'a mut Save,
    off: u64,
    owner: u64,
    depth: u32,
}

impl<'a> TableMut<'a> {
    pub(crate) fn new(save: &'a mut Save, off: u64, owner: u64, depth: u32) -> TableMut<'a> {
        TableMut {
            save,
            off,
            owner,
            depth,
        }
    }

    pub fn as_ref(&self) -> TableRef<'_> {
        TableRef::new(self.save, self.off)
    }

    pub fn get(&self, name: &str) -> Option<ValueRef<'_>> {
        self.as_ref().get(name)
    }

    fn field_cell(&self, i: u64) -> Result<u64, Error> {
        add(at(self.off, i, FIELD_LEN)?, FIELD_CELL)
    }

    /// Sets `name` to `value`, inserting the field when it is missing.
    pub fn set(&mut self, name: &str, value: Scalar<'_>) -> Result<(), Error> {
        if name.is_empty() {
            return Err(Error::BadName);
        }
        match self.as_ref().find(name) {
            Ok(i) => {
                let cell = self.field_cell(i)?;
                self.save.store(cell, value)
            }
            Err(pos) => {
                let (ty, v) = self.save.encode(value)?;
                self.insert(pos, name, ty, v).map(drop)
            }
        }
    }

    /// Inserts a field at sorted position `pos`; returns its cell.
    fn insert(&mut self, pos: u64, name: &str, ty: u16, v: u64) -> Result<u64, Error> {
        let fields = self.save.u64_at(self.off);
        if fields >= self.save.u64_at(add(self.off, 8)?) {
            self.off = self.save.grow(self.off, self.owner, FIELD_LEN)?;
        }
        let name_len = len_u64(name.len())?;
        let name_off = self.save.alloc(name_len)?;
        self.save.put_bytes(name_off, name.as_bytes())?;

        let slot = at(self.off, pos, FIELD_LEN)?;
        let live_end = at(self.off, fields, FIELD_LEN)?;
        let tail = live_end.checked_sub(slot).ok_or(Error::OutOfBounds)?;
        self.save.copy(slot, add(slot, FIELD_LEN)?, tail)?;
        self.save.put_u64(slot, name_off)?;
        self.save.put_u64(add(slot, 8)?, name_len)?;
        let cell = add(slot, FIELD_CELL)?;
        self.save.write_cell(cell, ty, v)?;
        self.save.put_u64(self.off, add(fields, 1)?)?;
        Ok(cell)
    }

    /// Removes `name`; returns whether it was present. Its data becomes garbage.
    pub fn remove(&mut self, name: &str) -> bool {
        match self.as_ref().find(name) {
            Ok(i) => self.remove_at(i).is_ok(),
            Err(_) => false,
        }
    }

    fn remove_at(&mut self, i: u64) -> Result<(), Error> {
        let fields = self.save.u64_at(self.off);
        let slot = at(self.off, i, FIELD_LEN)?;
        let next = add(slot, FIELD_LEN)?;
        let live_end = at(self.off, fields, FIELD_LEN)?;
        let tail = live_end.checked_sub(next).ok_or(Error::OutOfBounds)?;
        self.save.copy(next, slot, tail)?;
        let last = live_end.checked_sub(FIELD_LEN).ok_or(Error::OutOfBounds)?;
        self.save.fill_zero(last, FIELD_LEN)?;
        self.save.put_u64(self.off, fields.saturating_sub(1))
    }

    /// The `(cell, node)` of the container field `name`, created empty when missing.
    fn child(&mut self, name: &str, ty: ValueType) -> Result<(u64, u64), Error> {
        match self.as_ref().find(name) {
            Ok(i) => {
                let cell = self.field_cell(i)?;
                let (id, off) = self.save.cell(cell);
                if id != ty.id() {
                    return Err(Error::TypeMismatch);
                }
                Ok((cell, off))
            }
            Err(pos) => {
                if name.is_empty() {
                    return Err(Error::BadName);
                }
                let off = self.save.new_node(item_len(ty), DEFAULT_CAP)?;
                Ok((self.insert(pos, name, ty.id(), off)?, off))
            }
        }
    }

    /// The table field `name`, created empty when missing.
    pub fn table(&mut self, name: &str) -> Result<TableMut<'_>, Error> {
        let depth = child_depth(self.depth)?;
        let (cell, off) = self.child(name, ValueType::Table)?;
        Ok(TableMut::new(self.save, off, add(cell, CELL_VALUE)?, depth))
    }

    /// The array field `name`, created empty when missing.
    pub fn array(&mut self, name: &str) -> Result<ArrayMut<'_>, Error> {
        let depth = child_depth(self.depth)?;
        let (cell, off) = self.child(name, ValueType::Array)?;
        Ok(ArrayMut {
            save: self.save,
            off,
            owner: add(cell, CELL_VALUE)?,
            depth,
        })
    }
}

/// A mutable array handle.
pub struct ArrayMut<'a> {
    save: &'a mut Save,
    off: u64,
    owner: u64,
    depth: u32,
}

impl ArrayMut<'_> {
    pub fn as_ref(&self) -> ArrayRef<'_> {
        ArrayRef::new(self.save, self.off)
    }

    pub fn len(&self) -> u64 {
        self.save.u64_at(self.off)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, i: u64) -> Option<ValueRef<'_>> {
        self.as_ref().get(i)
    }

    /// The cell of element `i`, or `IndexOutOfRange`.
    fn elem(&self, i: u64) -> Result<u64, Error> {
        if i >= self.len() {
            return Err(Error::IndexOutOfRange);
        }
        at(self.off, i, ELEM_LEN)
    }

    pub fn push(&mut self, value: Scalar<'_>) -> Result<(), Error> {
        let (ty, v) = self.save.encode(value)?;
        self.save
            .push_cell(&mut self.off, self.owner, ty, v)
            .map(drop)
    }

    /// Replaces element `i`.
    pub fn set(&mut self, i: u64, value: Scalar<'_>) -> Result<(), Error> {
        let cell = self.elem(i)?;
        self.save.store(cell, value)
    }

    /// Shortens the array to `len` elements; does nothing when it is not shorter.
    pub fn truncate(&mut self, len: u64) {
        let old = self.len();
        if len >= old {
            return;
        }
        if let (Some(cut), Some(end)) =
            (item(self.off, len, ELEM_LEN), item(self.off, old, ELEM_LEN))
            && self.save.fill_zero(cut, end.saturating_sub(cut)).is_ok()
        {
            // The count lies before the zeroed cells, so this cannot fail after them.
            let _ = self.save.put_u64(self.off, len);
        }
    }

    fn push_node(&mut self, ty: ValueType) -> Result<(u64, u32), Error> {
        let depth = child_depth(self.depth)?;
        let node = self.save.new_node(item_len(ty), DEFAULT_CAP)?;
        let cell = self
            .save
            .push_cell(&mut self.off, self.owner, ty.id(), node)?;
        Ok((cell, depth))
    }

    /// Appends an empty table and returns it.
    pub fn push_table(&mut self) -> Result<TableMut<'_>, Error> {
        let (cell, depth) = self.push_node(ValueType::Table)?;
        let off = self.save.cell(cell).1;
        Ok(TableMut::new(self.save, off, add(cell, CELL_VALUE)?, depth))
    }

    /// Appends an empty array and returns it.
    pub fn push_array(&mut self) -> Result<ArrayMut<'_>, Error> {
        let (cell, depth) = self.push_node(ValueType::Array)?;
        self.array_cell(cell, depth)
    }

    fn array_cell(&mut self, cell: u64, depth: u32) -> Result<ArrayMut<'_>, Error> {
        Ok(ArrayMut {
            off: self.save.cell(cell).1,
            save: self.save,
            owner: add(cell, CELL_VALUE)?,
            depth,
        })
    }

    /// The `(cell, depth)` of element `i` if it is a `ty` container.
    fn child_at(&self, i: u64, ty: ValueType) -> Result<(u64, u32), Error> {
        let cell = self.elem(i)?;
        if self.save.cell(cell).0 != ty.id() {
            return Err(Error::TypeMismatch);
        }
        Ok((cell, child_depth(self.depth)?))
    }

    /// The table at element `i`.
    pub fn table_at(&mut self, i: u64) -> Result<TableMut<'_>, Error> {
        let (cell, depth) = self.child_at(i, ValueType::Table)?;
        let off = self.save.cell(cell).1;
        Ok(TableMut::new(self.save, off, add(cell, CELL_VALUE)?, depth))
    }

    /// The array at element `i`.
    pub fn array_at(&mut self, i: u64) -> Result<ArrayMut<'_>, Error> {
        let (cell, depth) = self.child_at(i, ValueType::Array)?;
        self.array_cell(cell, depth)
    }
}

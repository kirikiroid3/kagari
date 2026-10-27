//! Copying the live data into a fresh save.

use super::layout::{
    CELL_VALUE, ELEM_LEN, END_PTR, FIELD_CELL, FIELD_LEN, HEADER_END, MAGIC, PAGE_SIZE, ROOT_PTR,
    ValueType,
};
use super::write::{add, at, child_depth, item_len};
use super::{Error, Save};
use crate::le;

impl Save {
    /// A fresh save with the same content and the root back at offset 48. Every table and
    /// array keeps its capacity, its field order and its values; garbage is dropped.
    pub fn compact(&self) -> Save {
        // A `Save` is always well-formed and the copy is no larger than the live data, so this
        // cannot fail; an uncompacted copy is still the same content if it ever did.
        self.compacted().unwrap_or_else(|_| self.clone())
    }

    fn compacted(&self) -> Result<Save, Error> {
        let root = self.u64_at(ROOT_PTR);
        let mut out = Save {
            bytes: vec![0; usize::try_from(PAGE_SIZE).map_err(|_| Error::TooLarge)?],
        };
        out.put_bytes(0, &MAGIC)?;
        out.put_u64(ROOT_PTR, HEADER_END)?;
        out.put_u64(END_PTR, HEADER_END)?;
        let new_root = out.new_node(FIELD_LEN, self.u64_at(add(root, 8)?))?;
        out.copy_table(self, root, new_root, 0)?;
        Ok(out)
    }

    /// Copies the fields of `src`'s table at `from` into the empty table at `to`.
    fn copy_table(&mut self, src: &Save, from: u64, to: u64, depth: u32) -> Result<(), Error> {
        let fields = src.u64_at(from);
        for i in 0..fields {
            let (s, d) = (at(from, i, FIELD_LEN)?, at(to, i, FIELD_LEN)?);
            let name_len = src.u64_at(add(s, 8)?);
            let name =
                le::slice_at(&src.bytes, src.u64_at(s), name_len).ok_or(Error::OutOfBounds)?;
            let name_off = self.alloc(name_len)?;
            self.put_bytes(name_off, name)?;
            self.put_u64(d, name_off)?;
            self.put_u64(add(d, 8)?, name_len)?;
            self.copy_cell(src, add(s, FIELD_CELL)?, add(d, FIELD_CELL)?, depth)?;
        }
        self.put_u64(to, fields)
    }

    /// Copies the elements of `src`'s array at `from` into the empty array at `to`.
    fn copy_array(&mut self, src: &Save, from: u64, to: u64, depth: u32) -> Result<(), Error> {
        let len = src.u64_at(from);
        for i in 0..len {
            self.copy_cell(src, at(from, i, ELEM_LEN)?, at(to, i, ELEM_LEN)?, depth)?;
        }
        self.put_u64(to, len)
    }

    /// Deep-copies the cell at `from` in `src` (inside a container at `depth`) into `to`.
    fn copy_cell(&mut self, src: &Save, from: u64, to: u64, depth: u32) -> Result<(), Error> {
        let (id, v) = src.cell(from);
        let ty = ValueType::from_id(id).ok_or(Error::UnknownType(id))?;
        let value = match ty {
            ValueType::Null | ValueType::Bool | ValueType::I64 | ValueType::U64 => v,
            ValueType::I128 | ValueType::U128 | ValueType::Decimal | ValueType::Rat => {
                let len = ty.fixed_len().ok_or(Error::TypeMismatch)?;
                let data = le::slice_at(&src.bytes, v, len).ok_or(Error::OutOfBounds)?;
                let head = self.alloc(len)?;
                self.put_bytes(head, data)?;
                head
            }
            ValueType::String | ValueType::Bytes => {
                self.alloc_data(src.data(v).ok_or(Error::OutOfBounds)?)?
            }
            ValueType::Table | ValueType::Array => {
                let depth = child_depth(depth)?;
                let node = self.new_node(item_len(ty), src.u64_at(add(v, 8)?))?;
                if ty == ValueType::Table {
                    self.copy_table(src, v, node, depth)?;
                } else {
                    self.copy_array(src, v, node, depth)?;
                }
                node
            }
        };
        self.put_u16(to, id)?;
        self.put_u64(add(to, CELL_VALUE)?, value)
    }
}

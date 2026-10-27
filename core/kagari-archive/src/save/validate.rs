//! Eager validation of a loaded save. After it passes, every pointer a reader or writer
//! follows is in bounds and no two references share bytes, so in-place edits are safe.

use std::collections::HashSet;

use super::Error;
use super::layout::{
    CELL_VALUE, ELEM_LEN, END_PTR, FIELD_CELL, FIELD_LEN, HEAD_LEN, HEADER_END, MAGIC, MAX_DEPTH,
    PAGE_SIZE, ROOT_PTR, ValueType, item,
};
use crate::le;

pub(crate) fn validate(b: &[u8]) -> Result<(), Error> {
    let len = u64::try_from(b.len()).map_err(|_| Error::BadSize)?;
    if len == 0 || !len.is_multiple_of(PAGE_SIZE) {
        return Err(Error::BadSize);
    }
    let magic = b.get(..16).ok_or(Error::BadSize)?;
    if magic != MAGIC {
        return Err(if magic[..13] == MAGIC[..13] {
            Error::UnsupportedVersion
        } else {
            Error::BadMagic
        });
    }
    let field = |off| le::u64_at(b, off).ok_or(Error::BadSize);
    if field(32)? != 0 || field(40)? != 0 {
        return Err(Error::NonZeroReserved);
    }
    let root = field(ROOT_PTR)?;
    let end = field(END_PTR)?;
    if end > len || end < HEADER_END || root < HEADER_END {
        return Err(Error::OutOfBounds);
    }
    let mut v = Validator {
        b,
        end,
        nodes: HashSet::new(),
        regions: Vec::new(),
    };
    v.table(root, 0)?;
    v.disjoint()
}

struct Validator<'a> {
    b: &'a [u8],
    end: u64,
    /// Table and array heads already visited.
    nodes: HashSet<u64>,
    /// Every non-empty referenced region, as `start..end`.
    regions: Vec<(u64, u64)>,
}

impl<'a> Validator<'a> {
    fn u64(&self, off: u64) -> Result<u64, Error> {
        le::u64_at(self.b, off).ok_or(Error::OutOfBounds)
    }

    /// Checks that `len` bytes at `off` lie within `[48, end)` and records them.
    fn region(&mut self, off: u64, len: Option<u64>) -> Result<&'a [u8], Error> {
        let len = len.ok_or(Error::OutOfBounds)?;
        let stop = off
            .checked_add(len)
            .filter(|&stop| off >= HEADER_END && stop <= self.end)
            .ok_or(Error::OutOfBounds)?;
        if len > 0 {
            self.regions.push((off, stop));
        }
        le::slice_at(self.b, off, len).ok_or(Error::OutOfBounds)
    }

    /// Enters the table or array at `off`: checks depth and sharing, and returns `count`.
    fn node(&mut self, off: u64, depth: u32, item_len: u64) -> Result<u64, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::TooDeep);
        }
        if !self.nodes.insert(off) {
            return Err(Error::SharedNode);
        }
        self.region(off, Some(HEAD_LEN))?;
        let count = self.u64(off)?;
        let cap = self.u64(off.saturating_add(8))?;
        if count > cap {
            return Err(Error::OutOfBounds);
        }
        // Re-record the head with its items as one region.
        self.regions.pop();
        self.region(
            off,
            cap.checked_mul(item_len)
                .and_then(|n| n.checked_add(HEAD_LEN)),
        )?;
        Ok(count)
    }

    fn table(&mut self, off: u64, depth: u32) -> Result<(), Error> {
        let fields = self.node(off, depth, FIELD_LEN)?;
        let mut prev: Option<&[u8]> = None;
        for i in 0..fields {
            let slot = item(off, i, FIELD_LEN).ok_or(Error::OutOfBounds)?;
            let name_off = self.u64(slot)?;
            let name_len = self.u64(slot.saturating_add(8))?;
            if name_len == 0 {
                return Err(Error::BadName);
            }
            let name = self.region(name_off, Some(name_len))?;
            std::str::from_utf8(name).map_err(|_| Error::BadUtf8)?;
            if prev.is_some_and(|p| p >= name) {
                return Err(Error::NotSorted);
            }
            prev = Some(name);
            self.cell(slot.saturating_add(FIELD_CELL), depth)?;
        }
        Ok(())
    }

    fn array(&mut self, off: u64, depth: u32) -> Result<(), Error> {
        let len = self.node(off, depth, ELEM_LEN)?;
        for i in 0..len {
            self.cell(item(off, i, ELEM_LEN).ok_or(Error::OutOfBounds)?, depth)?;
        }
        Ok(())
    }

    /// Checks the cell at `cell` of a container at `depth`.
    fn cell(&mut self, cell: u64, depth: u32) -> Result<(), Error> {
        let id = le::u16_at(self.b, cell).ok_or(Error::OutOfBounds)?;
        let value = self.u64(cell.saturating_add(CELL_VALUE))?;
        let ty = ValueType::from_id(id).ok_or(Error::UnknownType(id))?;
        match ty {
            ValueType::Null if value != 0 => return Err(Error::BadValue),
            ValueType::Bool if value > 1 => return Err(Error::BadValue),
            ValueType::Null | ValueType::Bool | ValueType::I64 | ValueType::U64 => {}
            ValueType::I128 | ValueType::U128 | ValueType::Decimal | ValueType::Rat => {
                self.region(value, ty.fixed_len())?;
            }
            ValueType::String | ValueType::Bytes => {
                self.region(value, Some(HEAD_LEN))?;
                let data_off = self.u64(value)?;
                let data_len = self.u64(value.saturating_add(8))?;
                let data = self.region(data_off, Some(data_len))?;
                if ty == ValueType::String && std::str::from_utf8(data).is_err() {
                    return Err(Error::BadUtf8);
                }
            }
            ValueType::Table => self.table(value, depth.saturating_add(1))?,
            ValueType::Array => self.array(value, depth.saturating_add(1))?,
        }
        Ok(())
    }

    /// Rejects overlapping regions: an in-place edit through one would corrupt the other.
    fn disjoint(mut self) -> Result<(), Error> {
        self.regions.sort_unstable();
        if self.regions.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err(Error::SharedNode);
        }
        Ok(())
    }
}

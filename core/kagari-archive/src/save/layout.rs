//! The `.kisave` V000 layout constants and value type ids.

/// The first 16 bytes of every `.kisave`.
pub const MAGIC: [u8; 16] = *b"SAVEKAGARI\0\0V000";

/// The file length is a non-zero multiple of this.
pub const PAGE_SIZE: u64 = 16384;

/// The slot capacity of a fresh root table.
pub const ROOT_CAP: u64 = 128;

/// The capacity of tables and arrays the API creates.
pub const DEFAULT_CAP: u64 = 8;

/// The deepest nesting allowed; the root table is depth 0.
pub const MAX_DEPTH: u32 = 128;

/// The header's root pointer: an 8-byte pointer slot like a cell's value.
pub(crate) const ROOT_PTR: u64 = 16;
/// The header's bump pointer.
pub(crate) const END_PTR: u64 = 24;
/// The first byte after the header.
pub(crate) const HEADER_END: u64 = 48;
/// `end` of a fresh save: the header, the root head and its slots.
pub(crate) const INITIAL_END: u64 = 3392;
const _: () = assert!(INITIAL_END == HEADER_END + HEAD_LEN + ROOT_CAP * FIELD_LEN);

/// A table or array head: `(count, cap)`.
pub(crate) const HEAD_LEN: u64 = 16;
/// A table field slot: `name_off: u64`, `name_len: u64`, then a cell.
pub(crate) const FIELD_LEN: u64 = 26;
/// Where a field slot's cell starts.
pub(crate) const FIELD_CELL: u64 = 16;
/// An array element: one cell.
pub(crate) const ELEM_LEN: u64 = 10;
/// A cell is `type: u16, value: u64`; this is where `value` starts.
pub(crate) const CELL_VALUE: u64 = 2;

/// The type id stored in a cell. Inline types keep their value in the cell's 8-byte `value`;
/// the others store the offset of a head whose size the type decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum ValueType {
    /// Inline, value 0.
    Null = 0,
    /// Inline, 0 or 1.
    Bool = 1,
    /// Inline, the `i64` bits.
    I64 = 2,
    /// Inline.
    U64 = 3,
    /// → 16 bytes.
    I128 = 4,
    /// → 16 bytes.
    U128 = 5,
    /// → `mantissa: i128, scale: u32`, meaning `mantissa / 10^scale`.
    Decimal = 6,
    /// → `num: i128, den: i128`, stored raw.
    Rat = 7,
    /// → `(offset: u64, len: u64)` → UTF-8 bytes.
    String = 8,
    /// → `(offset: u64, len: u64)` → bytes.
    Bytes = 9,
    /// → table head.
    Table = 10,
    /// → array head.
    Array = 11,
}

impl ValueType {
    pub const fn id(self) -> u16 {
        self as u16
    }

    pub const fn from_id(id: u16) -> Option<ValueType> {
        Some(match id {
            0 => ValueType::Null,
            1 => ValueType::Bool,
            2 => ValueType::I64,
            3 => ValueType::U64,
            4 => ValueType::I128,
            5 => ValueType::U128,
            6 => ValueType::Decimal,
            7 => ValueType::Rat,
            8 => ValueType::String,
            9 => ValueType::Bytes,
            10 => ValueType::Table,
            11 => ValueType::Array,
            _ => return None,
        })
    }

    /// The size of a fixed head this type points to and rewrites in place, or `None`.
    pub(crate) const fn fixed_len(self) -> Option<u64> {
        match self {
            ValueType::I128 | ValueType::U128 => Some(16),
            ValueType::Decimal => Some(20),
            ValueType::Rat => Some(32),
            _ => None,
        }
    }
}

/// The offset of item `i` of the table or array at `off`.
pub(crate) fn item(off: u64, i: u64, item_len: u64) -> Option<u64> {
    i.checked_mul(item_len)?
        .checked_add(HEAD_LEN)?
        .checked_add(off)
}

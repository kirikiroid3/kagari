//! Why loading or editing a `.kisave` fails.

use std::fmt;

/// Why a save does not load, or an edit is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The length is not a non-zero multiple of the page size.
    BadSize,
    /// The file does not start with the `.kisave` magic.
    BadMagic,
    /// The magic matches except for the version digits.
    UnsupportedVersion,
    /// A reserved header field is not zero.
    NonZeroReserved,
    /// A pointer, a length or a region leaves `[48, end)`, or a count exceeds its capacity.
    OutOfBounds,
    /// A field name or a String is not UTF-8.
    BadUtf8,
    /// A field name is empty.
    BadName,
    UnknownType(u16),
    /// A Bool that is not 0 or 1, or a Null that is not 0.
    BadValue,
    /// Field names are not strictly ascending.
    NotSorted,
    /// Two references reach the same node, or two regions overlap.
    SharedNode,
    /// Nesting exceeds [`MAX_DEPTH`](super::MAX_DEPTH).
    TooDeep,
    /// The value has a different type than the operation needs.
    TypeMismatch,
    /// An array index is not below the array's length.
    IndexOutOfRange,
    /// The save would exceed the address space.
    TooLarge,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadSize => f.write_str("the save length is not a multiple of the page size"),
            Error::BadMagic => f.write_str("not a .kisave file (bad magic)"),
            Error::UnsupportedVersion => {
                f.write_str("unsupported .kisave format version (expected V000)")
            }
            Error::NonZeroReserved => f.write_str("a reserved field is not zero"),
            Error::OutOfBounds => f.write_str("a pointer or length is out of bounds"),
            Error::BadUtf8 => f.write_str("a string is not valid UTF-8"),
            Error::BadName => f.write_str("a field name is empty"),
            Error::UnknownType(id) => write!(f, "unknown value type {id}"),
            Error::BadValue => f.write_str("an invalid inline value"),
            Error::NotSorted => f.write_str("field names are not in ascending order"),
            Error::SharedNode => f.write_str("two references share the same data"),
            Error::TooDeep => f.write_str("values are nested too deeply"),
            Error::TypeMismatch => f.write_str("the value has a different type"),
            Error::IndexOutOfRange => f.write_str("the array index is out of range"),
            Error::TooLarge => f.write_str("the save is too large"),
        }
    }
}

impl std::error::Error for Error {}

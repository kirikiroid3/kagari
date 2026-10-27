//! Why reading, building or chaining a `.kiarchive` fails.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Why an archive operation fails.
#[derive(Debug)]
pub enum Error {
    /// The file does not start with the `.kiarchive` magic.
    BadMagic,
    /// The magic matches except for the version digits.
    UnsupportedVersion,
    /// The header, the index or a block ends early.
    Truncated,
    /// A reserved field or the header flags are not zero.
    NonZeroReserved,
    /// The index region does not match its CRC32.
    IndexCrc,
    /// A block's stored bytes do not match its CRC32.
    BlockCrc {
        block: u32,
    },
    /// An entry name breaks the name rule.
    BadName(String),
    /// Entry names are not strictly ascending.
    NotSorted,
    /// A block record is malformed: a non-contiguous offset, or lengths the codec cannot
    /// produce.
    BadBlock {
        block: u32,
    },
    /// An entry record is malformed: its block is out of range, its range leaves the block,
    /// its name lies outside the pool, or a tombstone has a nonzero range.
    BadEntry {
        index: u64,
    },
    /// The file length does not end exactly at the last block.
    FileLength {
        expected: u64,
        found: u64,
    },
    /// A base archive (sequence 0) contains a tombstone.
    TombstoneInBase,
    /// A block uses a codec this reader does not know.
    UnsupportedCodec(u16),
    /// A block's stored bytes do not decompress to its raw length.
    Decompress {
        block: u32,
    },
    /// The same name was added twice to a builder.
    DuplicateName(String),
    /// `delete` was called on a base archive builder.
    DeleteInBase,
    /// A chain has no layers.
    EmptyChain,
    /// A chain layer has the wrong sequence number.
    ChainOrder {
        expected: u32,
        found: u32,
    },
    /// A patch's `parent_crc` does not match the previous layer's `index_crc`.
    ChainParent {
        sequence: u32,
    },
    /// The parent's sequence number is `u32::MAX`.
    SequenceOverflow,
    /// A source file's size changed between `add_file` and `finish`.
    SourceChanged(PathBuf),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic => f.write_str("not a .kiarchive file (bad magic)"),
            Error::UnsupportedVersion => {
                f.write_str("unsupported .kiarchive format version (expected V000)")
            }
            Error::Truncated => f.write_str("the archive is truncated"),
            Error::NonZeroReserved => f.write_str("a reserved field is not zero"),
            Error::IndexCrc => f.write_str("the index checksum does not match"),
            Error::BlockCrc { block } => write!(f, "the checksum of block {block} does not match"),
            Error::BadName(name) => write!(f, "invalid entry name {name:?}"),
            Error::NotSorted => f.write_str("entry names are not in ascending order"),
            Error::BadBlock { block } => write!(f, "block {block} is malformed"),
            Error::BadEntry { index } => write!(f, "entry {index} is malformed"),
            Error::FileLength { expected, found } => {
                write!(f, "the file is {found} bytes long, expected {expected}")
            }
            Error::TombstoneInBase => f.write_str("a base archive contains a deletion"),
            Error::UnsupportedCodec(id) => write!(f, "unsupported codec {id}"),
            Error::Decompress { block } => write!(f, "block {block} does not decompress"),
            Error::DuplicateName(name) => write!(f, "entry {name:?} was added twice"),
            Error::DeleteInBase => f.write_str("a base archive cannot delete entries"),
            Error::EmptyChain => f.write_str("an archive chain needs at least one layer"),
            Error::ChainOrder { expected, found } => {
                write!(f, "expected archive layer {expected}, found {found}")
            }
            Error::ChainParent { sequence } => {
                write!(f, "patch {sequence} was not built on the previous layer")
            }
            Error::SequenceOverflow => f.write_str("the patch sequence number overflows"),
            Error::SourceChanged(path) => {
                write!(f, "{} changed size while building", path.display())
            }
            Error::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::Io(error)
    }
}

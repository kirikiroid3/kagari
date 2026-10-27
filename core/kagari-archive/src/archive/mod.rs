//! `.kiarchive`: an immutable asset package. Files are stored in compressed blocks, one per
//! file or several per solid block; the index sits at the front so a reader can stream it, and
//! numbered patch archives add, replace or delete files on top of a base.
//!
//! Layout (V000, little-endian): a 64-byte [`Header`], the index region (block table, entry
//! table sorted by name, name pool), then the blocks back to back.

mod build;
mod chain;
mod codec;
mod error;
mod index;
mod read;

#[cfg(test)]
mod tests;

pub use build::{ArchiveBuilder, ArchiveOptions};
pub use chain::{ArchiveChain, PatchedArchive, open_chain, patch_path};
pub use codec::Codec;
pub use error::Error;
pub use index::{ArchiveIndex, BlockInfo, FileLoc, Header, Lookup, decode_block};
pub use read::Archive;

/// The first 16 bytes of every `.kiarchive`.
pub const MAGIC: [u8; 16] = *b"ARCHKAGARI\0\0V000";

/// Length of the fixed header; the index region starts here.
pub const HEADER_LEN: u64 = 64;

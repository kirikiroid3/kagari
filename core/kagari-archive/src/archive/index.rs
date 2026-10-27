//! The header and the index region, parsed without any I/O. A network reader fetches the
//! header, then the index, then single blocks:
//!
//! 1. fetch `0..64` and call [`Header::parse`];
//! 2. fetch [`Header::index_range`] and call [`ArchiveIndex::parse`];
//! 3. [`ArchiveIndex::get`] a name, fetch [`BlockInfo::range`] of its block and call
//!    [`decode_block`];
//! 4. slice `loc.offset..loc.offset + loc.len` out of the raw block.

use std::ops::Range;

use super::codec::{self, Codec};
use super::{Error, HEADER_LEN, MAGIC};
use crate::le;

/// Size of one block record and of one entry record.
pub(crate) const RECORD_LEN: u64 = 32;

/// The entry `block` value that marks a deletion.
pub(crate) const TOMBSTONE: u32 = u32::MAX;

/// The largest raw/stored ratio an LZ4 block can claim; bounds allocations from a hostile index.
const LZ4_MAX_RATIO: u64 = 255;

/// The fixed 64-byte header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// 0 for a base archive, n ≥ 1 for patch n.
    pub sequence: u32,
    pub block_count: u64,
    pub entry_count: u64,
    /// Length of the index region, which starts at offset 64.
    pub index_len: u64,
    /// CRC32 of the index region.
    pub index_crc: u32,
    /// The parent layer's `index_crc`; 0 for a base.
    pub parent_crc: u32,
}

impl Header {
    /// Parses the first 64 bytes of `bytes`.
    pub fn parse(bytes: &[u8]) -> Result<Header, Error> {
        let b = bytes.get(..64).ok_or(Error::Truncated)?;
        if b[..16] != MAGIC {
            return Err(if b[..13] == MAGIC[..13] {
                Error::UnsupportedVersion
            } else {
                Error::BadMagic
            });
        }
        let field = |off| le::u64_at(b, off).ok_or(Error::Truncated);
        let field32 = |off| le::u32_at(b, off).ok_or(Error::Truncated);
        let header = Header {
            sequence: field32(16)?,
            block_count: field(24)?,
            entry_count: field(32)?,
            index_len: field(40)?,
            index_crc: field32(48)?,
            parent_crc: field32(52)?,
        };
        if field32(20)? != 0 || field(56)? != 0 {
            return Err(Error::NonZeroReserved);
        }
        // An index that would end past `u64::MAX` cannot be in any file.
        HEADER_LEN
            .checked_add(header.index_len)
            .ok_or(Error::Truncated)?;
        Ok(header)
    }

    /// The file range of the index region.
    pub fn index_range(&self) -> Range<u64> {
        HEADER_LEN..HEADER_LEN.saturating_add(self.index_len)
    }

    pub(crate) fn encode(&self) -> [u8; 64] {
        let mut b = [0; 64];
        b[..16].copy_from_slice(&MAGIC);
        b[16..20].copy_from_slice(&self.sequence.to_le_bytes());
        b[24..32].copy_from_slice(&self.block_count.to_le_bytes());
        b[32..40].copy_from_slice(&self.entry_count.to_le_bytes());
        b[40..48].copy_from_slice(&self.index_len.to_le_bytes());
        b[48..52].copy_from_slice(&self.index_crc.to_le_bytes());
        b[52..56].copy_from_slice(&self.parent_crc.to_le_bytes());
        b
    }
}

/// One block record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    /// Absolute file offset of the stored bytes.
    pub offset: u64,
    pub stored_len: u64,
    pub raw_len: u64,
    /// The raw codec id, so an index with an unknown codec still parses.
    pub codec: u16,
    /// CRC32 of the stored bytes.
    pub crc: u32,
}

impl BlockInfo {
    /// The file range of the stored bytes.
    pub fn range(&self) -> Range<u64> {
        self.offset..self.offset.saturating_add(self.stored_len)
    }
}

/// Where a live file's bytes are: a range of a block's raw bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileLoc {
    pub block: u32,
    pub offset: u64,
    pub len: u64,
}

/// What one layer says about a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookup {
    File(FileLoc),
    /// A patch deletes the name.
    Deleted,
}

#[derive(Clone, Debug)]
struct Entry {
    /// Range of the name in [`ArchiveIndex::names`].
    name: Range<usize>,
    lookup: Lookup,
}

/// A parsed and validated index region.
#[derive(Clone, Debug)]
pub struct ArchiveIndex {
    header: Header,
    blocks: Vec<BlockInfo>,
    entries: Vec<Entry>,
    /// The entry names, concatenated in entry order.
    names: String,
}

impl ArchiveIndex {
    /// Parses and validates the index region `index` described by `header`.
    pub fn parse(header: Header, index: &[u8]) -> Result<ArchiveIndex, Error> {
        if u64::try_from(index.len()).ok() != Some(header.index_len) {
            return Err(Error::Truncated);
        }
        if crc32fast::hash(index) != header.index_crc {
            return Err(Error::IndexCrc);
        }
        let blocks_len = header
            .block_count
            .checked_mul(RECORD_LEN)
            .ok_or(Error::Truncated)?;
        let records_len = header
            .entry_count
            .checked_mul(RECORD_LEN)
            .and_then(|e| e.checked_add(blocks_len))
            .filter(|&len| len <= header.index_len)
            .ok_or(Error::Truncated)?;
        // Ids up to `u32::MAX - 1`; `u32::MAX` is the tombstone.
        if header.block_count > u64::from(TOMBSTONE) {
            return Err(Error::BadBlock { block: TOMBSTONE });
        }

        let blocks = parse_blocks(&header, index)?;
        let pool = le::slice_at(
            index,
            records_len,
            header.index_len.saturating_sub(records_len),
        )
        .ok_or(Error::Truncated)?;
        let mut names = String::new();
        let mut entries = Vec::new();
        let mut prev: Option<&str> = None;
        let mut tombstone = false;
        let mut index_no = 0;
        while index_no < header.entry_count {
            let rec = index_no
                .checked_mul(RECORD_LEN)
                .and_then(|o| o.checked_add(blocks_len))
                .and_then(|o| le::slice_at(index, o, RECORD_LEN))
                .ok_or(Error::Truncated)?;
            let bad = Error::BadEntry { index: index_no };
            let (Some(name_off), Some(name_len), Some(block), Some(reserved)) = (
                le::u32_at(rec, 0),
                le::u32_at(rec, 4),
                le::u32_at(rec, 8),
                le::u32_at(rec, 12),
            ) else {
                return Err(Error::Truncated);
            };
            let (Some(offset), Some(len)) = (le::u64_at(rec, 16), le::u64_at(rec, 24)) else {
                return Err(Error::Truncated);
            };
            if reserved != 0 {
                return Err(Error::NonZeroReserved);
            }
            let raw = le::slice_at(pool, name_off.into(), name_len.into()).ok_or(bad)?;
            let name = std::str::from_utf8(raw)
                .map_err(|_| Error::BadName(String::from_utf8_lossy(raw).into_owned()))?;
            validate_name(name)?;
            if prev.is_some_and(|p| p >= name) {
                return Err(Error::NotSorted);
            }
            prev = Some(name);

            let lookup = if block == TOMBSTONE {
                if offset != 0 || len != 0 {
                    return Err(Error::BadEntry { index: index_no });
                }
                tombstone = true;
                Lookup::Deleted
            } else {
                let info = usize::try_from(block)
                    .ok()
                    .and_then(|b| blocks.get(b))
                    .ok_or(Error::BadEntry { index: index_no })?;
                if offset.checked_add(len).is_none_or(|end| end > info.raw_len) {
                    return Err(Error::BadEntry { index: index_no });
                }
                Lookup::File(FileLoc { block, offset, len })
            };
            let start = names.len();
            names.push_str(name);
            entries.push(Entry {
                name: start..names.len(),
                lookup,
            });
            index_no = index_no.saturating_add(1);
        }
        if header.sequence == 0 && tombstone {
            return Err(Error::TombstoneInBase);
        }
        if header.sequence == 0 && header.parent_crc != 0 {
            return Err(Error::NonZeroReserved);
        }
        Ok(ArchiveIndex {
            header,
            blocks,
            entries,
            names,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    fn name(&self, entry: &Entry) -> &str {
        self.names.get(entry.name.clone()).unwrap_or_default()
    }

    /// What this layer says about `name`, or `None` when it does not mention it.
    pub fn get(&self, name: &str) -> Option<Lookup> {
        let i = self
            .entries
            .binary_search_by(|e| self.name(e).cmp(name))
            .ok()?;
        self.entries.get(i).map(|e| e.lookup)
    }

    /// Every entry, in ascending name order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, Lookup)> {
        self.entries.iter().map(|e| (self.name(e), e.lookup))
    }

    pub fn block(&self, id: u32) -> Option<&BlockInfo> {
        self.blocks.get(usize::try_from(id).ok()?)
    }

    /// The file offset just past the last block: the expected file length.
    pub fn data_end(&self) -> u64 {
        self.blocks
            .last()
            .map_or(self.header.index_range().end, |b| b.range().end)
    }
}

fn parse_blocks(header: &Header, index: &[u8]) -> Result<Vec<BlockInfo>, Error> {
    let mut blocks = Vec::new();
    let mut expected = header.index_range().end;
    let mut id: u32 = 0;
    while u64::from(id) < header.block_count {
        let bad = Error::BadBlock { block: id };
        let rec = le::slice_at(index, u64::from(id).saturating_mul(RECORD_LEN), RECORD_LEN)
            .ok_or(Error::Truncated)?;
        let (Some(offset), Some(stored_len), Some(raw_len), Some(codec), Some(reserved), Some(crc)) = (
            le::u64_at(rec, 0),
            le::u64_at(rec, 8),
            le::u64_at(rec, 16),
            le::u16_at(rec, 24),
            le::u16_at(rec, 26),
            le::u32_at(rec, 28),
        ) else {
            return Err(Error::Truncated);
        };
        if reserved != 0 {
            return Err(Error::NonZeroReserved);
        }
        if offset != expected {
            return Err(bad);
        }
        expected = offset.checked_add(stored_len).ok_or(bad)?;
        let lengths_ok = match Codec::from_id(codec) {
            Some(Codec::Store) => raw_len == stored_len,
            Some(Codec::Lz4) => raw_len <= stored_len.saturating_mul(LZ4_MAX_RATIO),
            None => true,
        };
        if !lengths_ok {
            return Err(Error::BadBlock { block: id });
        }
        blocks.push(BlockInfo {
            offset,
            stored_len,
            raw_len,
            codec,
            crc,
        });
        id = id.saturating_add(1);
    }
    Ok(blocks)
}

/// Checks a `/`-separated relative entry name.
pub(crate) fn validate_name(name: &str) -> Result<(), Error> {
    let bad = name.is_empty()
        || u32::try_from(name.len()).is_err()
        || name.contains(['\\', '\0'])
        || name.starts_with('/')
        || name.ends_with('/')
        || name
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..");
    if bad {
        Err(Error::BadName(name.to_owned()))
    } else {
        Ok(())
    }
}

/// Checks and decodes block `id`'s stored bytes into its raw bytes.
pub fn decode_block(id: u32, info: &BlockInfo, stored: &[u8]) -> Result<Vec<u8>, Error> {
    if u64::try_from(stored.len()).ok() != Some(info.stored_len) {
        return Err(Error::Truncated);
    }
    if crc32fast::hash(stored) != info.crc {
        return Err(Error::BlockCrc { block: id });
    }
    let codec = Codec::from_id(info.codec).ok_or(Error::UnsupportedCodec(info.codec))?;
    let raw_len = usize::try_from(info.raw_len).map_err(|_| Error::Decompress { block: id })?;
    codec::decompress(codec, stored, raw_len).ok_or(Error::Decompress { block: id })
}

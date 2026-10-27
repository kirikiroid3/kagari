//! Building a base archive or a patch.

use std::collections::BTreeMap;
use std::collections::btree_map;
use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::num::NonZeroU64;
use std::path::PathBuf;

use super::codec::{self, Codec};
use super::index::{Header, RECORD_LEN, TOMBSTONE, validate_name};
use super::{Error, HEADER_LEN};

/// How a builder stores file bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveOptions {
    pub codec: Codec,
    /// The raw size a solid block closes at; `None` gives every file its own block, which
    /// keeps every file independently streamable.
    pub solid_block_size: Option<NonZeroU64>,
}

impl Default for ArchiveOptions {
    fn default() -> ArchiveOptions {
        ArchiveOptions {
            codec: Codec::Lz4,
            solid_block_size: None,
        }
    }
}

enum Pending {
    Bytes(Vec<u8>),
    File { path: PathBuf, len: u64 },
    Delete,
}

impl Pending {
    /// The raw length of a live entry; `None` for a deletion.
    fn len(&self) -> Option<u64> {
        match self {
            Pending::Bytes(b) => u64::try_from(b.len()).ok(),
            Pending::File { len, .. } => Some(*len),
            Pending::Delete => None,
        }
    }

    fn load(self) -> Result<Vec<u8>, Error> {
        match self {
            Pending::Bytes(b) => Ok(b),
            Pending::File { path, len } => {
                let data = fs::read(&path)?;
                if u64::try_from(data.len()).ok() != Some(len) {
                    return Err(Error::SourceChanged(path));
                }
                Ok(data)
            }
            Pending::Delete => Ok(Vec::new()),
        }
    }
}

/// Collects entries and writes them as one archive layer.
pub struct ArchiveBuilder {
    sequence: u32,
    parent_crc: u32,
    options: ArchiveOptions,
    entries: BTreeMap<String, Pending>,
}

fn too_large() -> Error {
    Error::Io(io::Error::other("the archive is too large"))
}

impl ArchiveBuilder {
    /// A builder for a base archive (sequence 0).
    pub fn base(options: ArchiveOptions) -> ArchiveBuilder {
        ArchiveBuilder {
            sequence: 0,
            parent_crc: 0,
            options,
            entries: BTreeMap::new(),
        }
    }

    /// A builder for the patch that follows the layer with header `parent`.
    pub fn patch(parent: &Header, options: ArchiveOptions) -> Result<ArchiveBuilder, Error> {
        Ok(ArchiveBuilder {
            sequence: parent
                .sequence
                .checked_add(1)
                .ok_or(Error::SequenceOverflow)?,
            parent_crc: parent.index_crc,
            options,
            entries: BTreeMap::new(),
        })
    }

    fn insert(&mut self, name: &str, pending: Pending) -> Result<(), Error> {
        validate_name(name)?;
        match self.entries.entry(name.to_owned()) {
            btree_map::Entry::Occupied(_) => Err(Error::DuplicateName(name.to_owned())),
            btree_map::Entry::Vacant(slot) => {
                slot.insert(pending);
                Ok(())
            }
        }
    }

    /// Adds (or, in a patch, replaces) `name` with `data`.
    pub fn add(&mut self, name: &str, data: Vec<u8>) -> Result<(), Error> {
        self.insert(name, Pending::Bytes(data))
    }

    /// Adds `name` with the contents of the file at `path`, read in [`finish`](Self::finish).
    /// The file's size is recorded now and must not change before then.
    pub fn add_file(&mut self, name: &str, path: impl Into<PathBuf>) -> Result<(), Error> {
        let path = path.into();
        let len = fs::metadata(&path)?.len();
        self.insert(name, Pending::File { path, len })
    }

    /// Deletes `name` from the layers below this patch.
    pub fn delete(&mut self, name: &str) -> Result<(), Error> {
        if self.sequence == 0 {
            return Err(Error::DeleteInBase);
        }
        self.insert(name, Pending::Delete)
    }

    /// Writes the archive to `out` and returns its header. Only one raw block is held in
    /// memory at a time.
    pub fn finish<W: Write + Seek>(self, mut out: W) -> Result<Header, Error> {
        let ArchiveBuilder {
            sequence,
            parent_crc,
            options,
            entries,
        } = self;

        // Group the live entries into blocks, in name order.
        // `records[i]` is `(block, offset, len)` of entry `i`.
        let mut records = Vec::with_capacity(entries.len());
        let mut blocks: Vec<u64> = Vec::new(); // the raw length of each block
        for pending in entries.values() {
            let Some(len) = pending.len() else {
                records.push((TOMBSTONE, 0, 0));
                continue;
            };
            // A solid block takes files until the next one would push it past the limit; a
            // file larger than the limit gets a block of its own.
            let open = match (options.solid_block_size, blocks.len().checked_sub(1)) {
                (Some(limit), Some(last)) => blocks.get_mut(last).filter(|cur| {
                    **cur == 0 || cur.checked_add(len).is_some_and(|end| end <= limit.get())
                }),
                _ => None,
            };
            if let Some(cur) = open {
                let at = *cur;
                *cur = at.checked_add(len).ok_or_else(too_large)?;
                records.push((block_id(blocks.len().saturating_sub(1))?, at, len));
            } else {
                records.push((block_id(blocks.len())?, 0, len));
                blocks.push(len);
            }
        }

        let mut pool = Vec::new();
        let mut name_refs = Vec::with_capacity(entries.len());
        for name in entries.keys() {
            let off = u32::try_from(pool.len()).map_err(|_| Error::BadName(name.clone()))?;
            let len = u32::try_from(name.len()).map_err(|_| Error::BadName(name.clone()))?;
            name_refs.push((off, len));
            pool.extend_from_slice(name.as_bytes());
        }
        pool.resize(pool.len().next_multiple_of(8), 0);

        let block_count = u64::try_from(blocks.len()).map_err(|_| too_large())?;
        let entry_count = u64::try_from(entries.len()).map_err(|_| too_large())?;
        let index_len = block_count
            .checked_add(entry_count)
            .and_then(|n| n.checked_mul(RECORD_LEN))
            .and_then(|n| n.checked_add(u64::try_from(pool.len()).ok()?))
            .ok_or_else(too_large)?;
        let data_start = HEADER_LEN.checked_add(index_len).ok_or_else(too_large)?;

        // Write the blocks.
        let mut index = Vec::with_capacity(usize::try_from(index_len).map_err(|_| too_large())?);
        out.seek(SeekFrom::Start(data_start))?;
        let mut offset = data_start;
        let mut sources = entries
            .into_values()
            .zip(records.iter().map(|r| r.0))
            .filter(|&(_, block)| block != TOMBSTONE)
            .peekable();
        for (id, &raw_len) in blocks.iter().enumerate() {
            let id = block_id(id)?;
            let mut raw = Vec::new();
            while let Some((pending, _)) = sources.next_if(|&(_, block)| block == id) {
                let data = pending.load()?;
                if raw.is_empty() {
                    raw = data;
                } else {
                    raw.extend_from_slice(&data);
                }
            }
            let (codec, stored) = match options.codec {
                Codec::Store => (Codec::Store, raw),
                codec => {
                    let stored = codec::compress(codec, &raw);
                    if stored.len() < raw.len() {
                        (codec, stored)
                    } else {
                        (Codec::Store, raw)
                    }
                }
            };
            let stored_len = u64::try_from(stored.len()).map_err(|_| too_large())?;
            out.write_all(&stored)?;
            index.extend_from_slice(&offset.to_le_bytes());
            index.extend_from_slice(&stored_len.to_le_bytes());
            index.extend_from_slice(&raw_len.to_le_bytes());
            index.extend_from_slice(&codec.id().to_le_bytes());
            index.extend_from_slice(&0u16.to_le_bytes());
            index.extend_from_slice(&crc32fast::hash(&stored).to_le_bytes());
            offset = offset.checked_add(stored_len).ok_or_else(too_large)?;
        }

        // Encode the entry table and the pool, then the header.
        for (&(name_off, name_len), &(block, off, len)) in name_refs.iter().zip(&records) {
            index.extend_from_slice(&name_off.to_le_bytes());
            index.extend_from_slice(&name_len.to_le_bytes());
            index.extend_from_slice(&block.to_le_bytes());
            index.extend_from_slice(&0u32.to_le_bytes());
            index.extend_from_slice(&off.to_le_bytes());
            index.extend_from_slice(&len.to_le_bytes());
        }
        index.extend_from_slice(&pool);
        let header = Header {
            sequence,
            block_count,
            entry_count,
            index_len,
            index_crc: crc32fast::hash(&index),
            parent_crc,
        };
        out.seek(SeekFrom::Start(0))?;
        out.write_all(&header.encode())?;
        out.write_all(&index)?;
        out.flush()?;
        Ok(header)
    }
}

/// A block number as stored in an entry; `u32::MAX` is reserved for tombstones.
fn block_id(i: usize) -> Result<u32, Error> {
    u32::try_from(i)
        .ok()
        .filter(|&b| b != TOMBSTONE)
        .ok_or_else(too_large)
}

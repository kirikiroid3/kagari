//! Reading a single `.kiarchive` from a seekable source.

use std::io::{self, Read, Seek, SeekFrom};

use super::index::{ArchiveIndex, FileLoc, Header, Lookup, decode_block};
use super::{Error, HEADER_LEN};
use crate::le;

/// The last decoded block: its id and raw bytes.
pub(crate) type BlockCache = Option<(u32, Vec<u8>)>;

/// An open archive. Reads decode whole blocks and keep the last one, so sequential reads from
/// one solid block decompress it once.
pub struct Archive<R> {
    index: ArchiveIndex,
    reader: R,
    cache: BlockCache,
}

impl<R: Read + Seek> Archive<R> {
    /// Reads and validates the header and the index, and checks the file length.
    pub fn open(mut reader: R) -> Result<Self, Error> {
        let file_len = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let mut head = [0; 64];
        read_exact(&mut reader, &mut head)?;
        let header = Header::parse(&head)?;
        if header.index_range().end > file_len {
            return Err(Error::Truncated);
        }
        let mut index = vec![0; usize::try_from(header.index_len).map_err(|_| Error::Truncated)?];
        reader.seek(SeekFrom::Start(HEADER_LEN))?;
        read_exact(&mut reader, &mut index)?;
        let index = ArchiveIndex::parse(header, &index)?;
        let expected = index.data_end();
        if file_len != expected {
            return Err(Error::FileLength {
                expected,
                found: file_len,
            });
        }
        Ok(Archive {
            index,
            reader,
            cache: None,
        })
    }

    pub fn index(&self) -> &ArchiveIndex {
        &self.index
    }

    /// The bytes of `name`, or `None` when it is absent or deleted.
    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        match self.index.get(name) {
            Some(Lookup::File(loc)) => {
                read_loc(&self.index, &mut self.reader, &mut self.cache, loc).map(Some)
            }
            Some(Lookup::Deleted) | None => Ok(None),
        }
    }

    pub(crate) fn into_parts(self) -> (ArchiveIndex, R, BlockCache) {
        (self.index, self.reader, self.cache)
    }
}

/// Reads the file at `loc`, decoding its block unless `cache` already holds it.
pub(crate) fn read_loc<R: Read + Seek>(
    index: &ArchiveIndex,
    reader: &mut R,
    cache: &mut BlockCache,
    loc: FileLoc,
) -> Result<Vec<u8>, Error> {
    let raw = match cache {
        Some((id, raw)) if *id == loc.block => raw,
        _ => {
            let info = index
                .block(loc.block)
                .ok_or(Error::BadBlock { block: loc.block })?;
            let mut stored =
                vec![0; usize::try_from(info.stored_len).map_err(|_| Error::Truncated)?];
            reader.seek(SeekFrom::Start(info.offset))?;
            read_exact(reader, &mut stored)?;
            let raw = decode_block(loc.block, info, &stored)?;
            &mut cache.insert((loc.block, raw)).1
        }
    };
    le::slice_at(raw, loc.offset, loc.len)
        .map(<[u8]>::to_vec)
        .ok_or(Error::BadBlock { block: loc.block })
}

/// `read_exact` with an early end reported as [`Error::Truncated`].
fn read_exact<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<(), Error> {
    reader.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => Error::Truncated,
        _ => Error::Io(e),
    })
}

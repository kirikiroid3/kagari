//! A base archive plus numbered patches (`cg.kiarchive`, `cg.001.kiarchive`, …). Each patch
//! adds, replaces or deletes files on top of the layers below it.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

use super::{ARCHIVE_EXTENSION, Error};
use super::index::{ArchiveIndex, FileLoc, Lookup};
use super::read::{Archive, BlockCache, read_loc};

/// Validated layer indexes, base first, without any I/O.
#[derive(Clone, Debug)]
pub struct ArchiveChain {
    layers: Vec<ArchiveIndex>,
}

impl ArchiveChain {
    /// Checks that layer `i` has sequence `i` and was built on layer `i - 1`.
    pub fn new(layers: Vec<ArchiveIndex>) -> Result<ArchiveChain, Error> {
        if layers.is_empty() {
            return Err(Error::EmptyChain);
        }
        let mut parent_crc = None;
        for (i, layer) in layers.iter().enumerate() {
            let expected = u32::try_from(i).unwrap_or(u32::MAX);
            let header = layer.header();
            if header.sequence != expected || u32::try_from(i).is_err() {
                return Err(Error::ChainOrder {
                    expected,
                    found: header.sequence,
                });
            }
            if parent_crc.is_some_and(|crc| crc != header.parent_crc) {
                return Err(Error::ChainParent { sequence: expected });
            }
            parent_crc = Some(header.index_crc);
        }
        Ok(ArchiveChain { layers })
    }

    /// The layer and location that provide `name`, or `None` when it is absent or deleted.
    pub fn resolve(&self, name: &str) -> Option<(usize, FileLoc)> {
        self.layers
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, layer)| layer.get(name).map(|lookup| (i, lookup)))
            .and_then(|(i, lookup)| match lookup {
                Lookup::File(loc) => Some((i, loc)),
                Lookup::Deleted => None,
            })
    }

    /// Every live name, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut live = BTreeSet::new();
        for layer in &self.layers {
            for (name, lookup) in layer.entries() {
                match lookup {
                    Lookup::File(_) => live.insert(name),
                    Lookup::Deleted => live.remove(name),
                };
            }
        }
        live.into_iter().collect()
    }

    pub fn layer(&self, i: usize) -> Option<&ArchiveIndex> {
        self.layers.get(i)
    }
}

/// An [`ArchiveChain`] over open readers.
pub struct PatchedArchive<R> {
    chain: ArchiveChain,
    readers: Vec<(R, BlockCache)>,
}

impl<R: Read + Seek> PatchedArchive<R> {
    /// Chains `layers`, base first.
    pub fn new(layers: Vec<Archive<R>>) -> Result<Self, Error> {
        let (indexes, readers) = layers
            .into_iter()
            .map(|archive| {
                let (index, reader, cache) = archive.into_parts();
                (index, (reader, cache))
            })
            .unzip();
        Ok(PatchedArchive {
            chain: ArchiveChain::new(indexes)?,
            readers,
        })
    }

    /// The bytes of `name` from the newest layer that mentions it, or `None` when it is absent
    /// or deleted.
    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some((i, loc)) = self.chain.resolve(name) else {
            return Ok(None);
        };
        let (Some(index), Some((reader, cache))) = (self.chain.layer(i), self.readers.get_mut(i))
        else {
            return Ok(None);
        };
        read_loc(index, reader, cache, loc).map(Some)
    }

    pub fn chain(&self) -> &ArchiveChain {
        &self.chain
    }
}

/// The path of patch `n` next to `base`: `cg.kiarchive`, 1 → `cg.001.kiarchive`. `None`
/// when `base`'s file name does not end in `.kiarchive`.
pub fn patch_path(base: &Path, n: u32) -> Option<PathBuf> {
    let stem = base
        .file_name()?
        .to_str()?
        .strip_suffix(ARCHIVE_EXTENSION)?
        .strip_suffix('.')
        .filter(|stem| !stem.is_empty())?;
    Some(base.with_file_name(format!("{stem}.{n:03}.{ARCHIVE_EXTENSION}")))
}

/// Opens `base` and its patches 1, 2, … up to the first missing one.
pub fn open_chain(base: &Path) -> Result<PatchedArchive<File>, Error> {
    if patch_path(base, 1).is_none() {
        return Err(Error::BadName(base.display().to_string()));
    }
    let mut layers = vec![Archive::open(File::open(base)?)?];
    let mut n: u32 = 1;
    while let Some(path) = patch_path(base, n) {
        match File::open(&path) {
            Ok(file) => layers.push(Archive::open(file)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.into()),
        }
        let Some(next) = n.checked_add(1) else { break };
        n = next;
    }
    PatchedArchive::new(layers)
}

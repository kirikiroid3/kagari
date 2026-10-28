//! Packs the assets the bytecode references into `.kiarchive` files: one per top-level
//! directory of the bytecode asset paths, and `data.kiarchive` for files directly in the asset
//! directory. An entry's name is the asset path in the bytecode, so system fonts, named
//! `system-fonts/…`, go into `system-fonts.kiarchive`, and std assets, named `std/…` and read
//! from the compiler's embedded std files, go into `std.kiarchive`.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use kagari_archive::archive::{ARCHIVE_EXTENSION, ArchiveBuilder, ArchiveOptions, archive_stem};
use kagari_bytecode::{Asset, Module};
use kagari_lang::FilePath;

use crate::failure::{Failure, io};

/// The bytecode path of `asset`.
pub fn asset_path<'m>(module: &'m Module, asset: &Asset) -> Result<&'m str, Failure> {
    usize::try_from(asset.path.0)
        .ok()
        .and_then(|index| module.strings.get(index))
        .map(String::as_str)
        .ok_or_else(|| Failure("the bytecode names an asset with no path".to_owned()))
}

/// Where the bytes of an asset come from.
pub enum Source<'f> {
    /// A file on disk.
    Disk(&'f Path),
    /// A file embedded in `kagari_std`.
    Std(&'static str),
}

/// The source of an asset whose file is `file`.
pub fn source(file: &FilePath) -> Result<Source<'_>, Failure> {
    match file {
        FilePath::Disk(file) => Ok(Source::Disk(file)),
        FilePath::Std(name) => kagari_std::file(name)
            .map(Source::Std)
            .ok_or_else(|| Failure(format!("std asset `{name}` is missing"))),
    }
}

/// Writes the archives into `dir`; `files` is the file of each entry of `module.assets`.
/// Returns each archive's path and file count.
pub fn write(
    module: &Module,
    files: &[FilePath],
    dir: &Path,
) -> Result<Vec<(PathBuf, usize)>, Failure> {
    let mut groups: BTreeMap<String, BTreeMap<String, Source<'_>>> = BTreeMap::new();
    for (asset, file) in module.assets.iter().zip(files) {
        let path = asset_path(module, asset)?;
        let source = source(file)?;
        groups
            .entry(archive_stem(path).to_owned())
            .or_default()
            .insert(path.to_owned(), source);
    }

    let mut written = Vec::new();
    for (stem, entries) in groups {
        let archive = dir.join(format!("{stem}.{ARCHIVE_EXTENSION}"));
        let failed = |error: kagari_archive::archive::Error| {
            Failure(format!("{}: {error}", archive.display()))
        };
        let mut builder = ArchiveBuilder::base(ArchiveOptions::default());
        for (entry, source) in &entries {
            match source {
                Source::Disk(file) => builder.add_file(entry, file).map_err(failed)?,
                Source::Std(text) => builder
                    .add(entry, text.as_bytes().to_vec())
                    .map_err(failed)?,
            }
        }
        let out = File::create(&archive).map_err(|error| io(&archive, error))?;
        builder.finish(out).map_err(failed)?;
        written.push((archive, entries.len()));
    }
    Ok(written)
}

//! The asset format check of `kagari build`: every asset the bytecode references is sniffed by
//! content and checked against each runtime profile in `build.runtime-compatibility`.

use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::Path;

use kagari_bytecode::Module;
use kagari_compat::{RuntimeProfile, describe, read_iso_bmff_head, sniff, supports, supports_kind};
use kagari_lang::FilePath;

use super::archives;
use crate::failure::{Failure, io};

/// Bytes read per asset for sniffing.
const SNIFF_LIMIT: u64 = 1 << 20;

/// Bytes read from an MP4/M4A file's boxes other than media data: its `moov` box, often written
/// after the media data, holds the sample descriptions.
const ISO_BMFF_LIMIT: u64 = 16 << 20;

/// Prints one error line per asset that a profile cannot decode; returns how many lines it printed.
pub fn check(
    module: &Module,
    files: &[FilePath],
    profiles: &[RuntimeProfile],
) -> Result<usize, Failure> {
    let mut errors: usize = 0;
    for (asset, file) in module.assets.iter().zip(files) {
        let path = archives::asset_path(module, asset)?;
        let head = match archives::source(file)? {
            archives::Source::Disk(disk) => read_head(disk).map_err(|error| io(disk, error))?,
            archives::Source::Std(text) => text.as_bytes().to_vec(),
        };
        let format = sniff(asset.kind, &head);
        let kind = asset.kind.name();
        for &profile in profiles {
            let profile_name = profile.as_str();
            if !supports_kind(profile, asset.kind) {
                eprintln!(
                    "error: asset `{path}` ({kind}): {kind} assets are not supported by runtime profile `{profile_name}`"
                );
            } else if !supports(profile, asset.kind, format) {
                eprintln!(
                    "error: asset `{path}` ({kind}): {} is not supported by runtime profile `{profile_name}`",
                    describe(format)
                );
            } else {
                continue;
            }
            errors = errors.saturating_add(1);
        }
    }
    Ok(errors)
}

/// The bytes `sniff` needs from the file at `path`: its first [`SNIFF_LIMIT`] bytes or, for an
/// ISO BMFF file (MP4, M4A, AVIF), its boxes other than media data.
fn read_head(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut head = Vec::new();
    file.by_ref().take(SNIFF_LIMIT).read_to_end(&mut head)?;
    if head.get(4..8) == Some(b"ftyp") {
        file.rewind()?;
        head = read_iso_bmff_head(&mut file, ISO_BMFF_LIMIT)?;
    }
    Ok(head)
}

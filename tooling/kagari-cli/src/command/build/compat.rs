//! The asset format check of `kagari build`: every asset the bytecode references is sniffed by
//! content and checked against each runtime profile in `build.runtime-compatibility`.

use std::fs::File;
use std::io::Read;

use kagari_bytecode::Module;
use kagari_compat::{RuntimeProfile, describe, sniff, supports, supports_kind};
use kagari_lang::FilePath;

use super::archives;
use crate::failure::{Failure, io};

/// Bytes read per asset for sniffing.
const SNIFF_LIMIT: u64 = 1 << 20;

/// Prints one error line per asset that a profile cannot decode; returns how many lines it printed.
pub fn check(
    module: &Module,
    files: &[FilePath],
    profiles: &[RuntimeProfile],
) -> Result<usize, Failure> {
    let mut errors: usize = 0;
    for (asset, file) in module.assets.iter().zip(files) {
        let path = archives::asset_path(module, asset)?;
        let disk = archives::disk_file(path, file)?;
        let mut head = Vec::new();
        File::open(disk)
            .and_then(|file| file.take(SNIFF_LIMIT).read_to_end(&mut head))
            .map_err(|error| io(disk, error))?;
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

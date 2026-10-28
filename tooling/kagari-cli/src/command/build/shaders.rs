//! The shader check of `kagari build`: every shader asset is validated against the WGSL prelude
//! of each role it is used under.

use kagari_bytecode::Module;
use kagari_lang::FilePath;
use kagari_shader::ShaderRole;

use super::archives;
use crate::failure::{Failure, io};

/// Prints one error line per invalid shader; returns whether there was any.
/// `roles` is parallel to `files` and `module.assets`.
pub fn check(
    module: &Module,
    files: &[FilePath],
    roles: &[Option<ShaderRole>],
) -> Result<bool, Failure> {
    let mut failed = false;
    for ((asset, file), role) in module.assets.iter().zip(files).zip(roles) {
        let Some(role) = *role else { continue };
        let path = archives::asset_path(module, asset)?;
        let source = match archives::source(file)? {
            archives::Source::Disk(disk) => {
                std::fs::read_to_string(disk).map_err(|error| io(disk, error))?
            }
            archives::Source::Std(text) => text.to_owned(),
        };
        if let Err(error) = kagari_shader::validate(role, &source) {
            eprintln!("error: shader `{path}` ({}): {error}", role.name());
            failed = true;
        }
    }
    Ok(failed)
}

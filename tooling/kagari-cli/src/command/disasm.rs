//! `kagari disasm`: a bytecode file as text.

use std::path::Path;
use std::process::ExitCode;

use crate::failure::{Failure, exit, io};

pub fn run(file: &Path, all: bool) -> Result<ExitCode, Failure> {
    let bytes = std::fs::read(file).map_err(|error| io(file, error))?;
    let module = match kagari_bytecode::decode(&bytes) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("{}: {error} ({error:?})", file.display());
            return Ok(exit(true));
        }
    };
    if let Err(error) = kagari_bytecode::verify(&module) {
        eprintln!("{}: {error}", file.display());
        return Ok(exit(true));
    }
    let filter = |path: &str| all || path.starts_with("crate::");
    print!("{}", kagari_bytecode::disassemble(&module, &filter));
    Ok(ExitCode::SUCCESS)
}

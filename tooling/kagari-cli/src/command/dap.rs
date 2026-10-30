//! `kagari dap`: the debug adapter of `kagari-debugger` over stdin/stdout, opening projects with
//! [`Projects`].

use std::process::ExitCode;

use crate::debugger::Projects;
use crate::failure::Failure;

pub fn run() -> Result<ExitCode, Failure> {
    kagari_debugger::dap::run_stdio(&mut Projects);
    Ok(ExitCode::SUCCESS)
}

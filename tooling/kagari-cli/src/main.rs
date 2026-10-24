#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]

//! The `kagari` command: check a program, build it to a `.kagari` module, or disassemble one.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use kagari_lang::debug::resolution_dump;
use kagari_lang::{
    CompileOptions, FilePath, OsFileSystem, RootDatabase, Severity, build, check,
    render_diagnostics,
};

const USAGE: &str = "usage: kagari check <root> [--resolutions] [opts] | kagari build <root> \
                     [-o <out>] [opts] | kagari disasm <file> [--all]; opts: --state-cap <n> \
                     --recursion-depth <n> --keyframe-check";

/// A command line that cannot run: a usage or IO error, reported with exit code 2.
struct Failure(String);

enum Command {
    Check {
        root: PathBuf,
        resolutions: bool,
        options: CompileOptions,
    },
    Build {
        root: PathBuf,
        out: Option<PathBuf>,
        options: CompileOptions,
    },
    Disasm {
        file: PathBuf,
        all: bool,
    },
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let result = parse(&args).and_then(run);
    match result {
        Ok(code) => code,
        Err(Failure(message)) => {
            eprintln!("kagari: {message}");
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn parse(args: &[OsString]) -> Result<Command, Failure> {
    let mut args = args.iter();
    let command = args
        .next()
        .ok_or_else(|| Failure("no command given".to_owned()))?;
    let command = command.to_string_lossy();
    if !matches!(command.as_ref(), "check" | "build" | "disasm") {
        return Err(Failure(format!("unknown command `{command}`")));
    }
    let mut path: Option<PathBuf> = None;
    let mut options = CompileOptions::default();
    let mut resolutions = false;
    let mut out: Option<PathBuf> = None;
    let mut all = false;
    let compiles = command == "check" || command == "build";
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        match text.as_ref() {
            "--state-cap" if compiles => options.state_cap = number(args.next(), &text)?,
            "--recursion-depth" if compiles => {
                options.recursion_depth = number(args.next(), &text)?;
            }
            "--keyframe-check" if compiles => options.keyframe_check = true,
            "--resolutions" if command == "check" => resolutions = true,
            "-o" if command == "build" => {
                let value = args
                    .next()
                    .ok_or_else(|| Failure("`-o` needs a path".to_owned()))?;
                out = Some(PathBuf::from(value));
            }
            "--all" if command == "disasm" => all = true,
            _ if text.starts_with('-') => {
                return Err(Failure(format!("unknown option `{text}` for `{command}`")));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err(Failure(format!("unexpected argument `{text}`"))),
        }
    }
    let path = path.ok_or_else(|| Failure(format!("`{command}` needs a file")))?;
    match command.as_ref() {
        "check" => Ok(Command::Check {
            root: path,
            resolutions,
            options,
        }),
        "build" => Ok(Command::Build {
            root: path,
            out,
            options,
        }),
        "disasm" => Ok(Command::Disasm { file: path, all }),
        _ => Err(Failure(format!("unknown command `{command}`"))),
    }
}

fn number(value: Option<&OsString>, option: &str) -> Result<u32, Failure> {
    let value = value.ok_or_else(|| Failure(format!("`{option}` needs a number")))?;
    value.to_string_lossy().parse().map_err(|_| {
        Failure(format!(
            "`{option}` needs a number, found `{}`",
            value.to_string_lossy()
        ))
    })
}

fn run(command: Command) -> Result<ExitCode, Failure> {
    let base = std::env::current_dir().ok();
    match command {
        Command::Check {
            root,
            resolutions,
            options,
        } => {
            let root = canonical(&root)?;
            let db = RootDatabase::new(Arc::new(OsFileSystem));
            let program = db.program_with(FilePath::disk(root), options);
            let diags = check(&db, program);
            print!("{}", render_diagnostics(&db, diags, base.as_deref()));
            if resolutions {
                print!("{}", resolution_dump(&db, program, base.as_deref()));
            }
            Ok(exit(
                diags.iter().any(|diag| diag.severity == Severity::Error),
            ))
        }
        Command::Build { root, out, options } => {
            let root = canonical(&root)?;
            let out = out.unwrap_or_else(|| root.with_extension("kagari"));
            let db = RootDatabase::new(Arc::new(OsFileSystem));
            let program = db.program_with(FilePath::disk(&root), options);
            let result = build(&db, program);
            if !result.diagnostics.is_empty() {
                eprint!(
                    "{}",
                    render_diagnostics(&db, &result.diagnostics, base.as_deref())
                );
            }
            let Some(bytes) = result.bytes else {
                return Ok(exit(true));
            };
            std::fs::write(&out, bytes)
                .map_err(|err| Failure(format!("{}: {err}", out.display())))?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Disasm { file, all } => {
            let bytes = std::fs::read(&file)
                .map_err(|err| Failure(format!("{}: {err}", file.display())))?;
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
    }
}

fn canonical(path: &Path) -> Result<PathBuf, Failure> {
    std::fs::canonicalize(path).map_err(|err| Failure(format!("{}: {err}", path.display())))
}

fn exit(failed: bool) -> ExitCode {
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
//! `mdbook-kagari`: an mdBook preprocessor that highlights `kis` and `kix` code blocks at build
//! time with the tree-sitter grammar (`tooling/tree-sitter-kagari`) and the Zed extension's
//! `highlights.scm`. See `docs/dev/src/process/docs.md`.
//!
//! Protocol: `mdbook-kagari supports <renderer>` exits 0 for `html` and 1 otherwise; without
//! arguments it reads `[context, book]` JSON from stdin and writes the book back to stdout.

mod chapter;
mod highlight;
#[cfg(test)]
mod tests;

use std::error::Error;
use std::io;
use std::process::ExitCode;

use highlight::Highlighter;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None => match run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mdbook-kagari: {error}");
                ExitCode::from(2)
            }
        },
        Some("supports") if args.next().as_deref() == Some("html") => ExitCode::SUCCESS,
        Some("supports") => ExitCode::FAILURE,
        Some(other) => {
            eprintln!("mdbook-kagari: unknown argument `{other}`");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let (_context, mut book) = mdbook_preprocessor::parse_input(io::stdin().lock())?;
    let mut highlighter = Highlighter::new()?;
    book.for_each_chapter_mut(|chapter| {
        chapter.content = chapter::highlight_code_blocks(&chapter.content, &mut highlighter);
    });
    serde_json::to_writer(io::stdout().lock(), &book)?;
    Ok(())
}

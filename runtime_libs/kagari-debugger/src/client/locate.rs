//! Source locations for the debugger front ends. A runtime names a location by the file the
//! bytecode's Debug section names (relative to the root file's directory, or `std/…`) and a
//! byte offset; the front ends show lines and columns, and turn a source line into the
//! breakpoints the runtime understands (step IDs, or code words of story functions).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use kagari_bytecode::{FnKind, Instr, Module, word_offsets};

use crate::protocol::InstructionPoint;

/// A source file with the start offset of each line.
struct Source {
    path: PathBuf,
    text: String,
    starts: Vec<usize>,
}

impl Source {
    fn new(path: PathBuf, text: String) -> Source {
        let starts = std::iter::once(0)
            .chain(
                text.match_indices('\n')
                    .filter_map(|(at, _)| at.checked_add(1)),
            )
            .collect();
        Source { path, text, starts }
    }

    /// The 0-based line and UTF-16 column of `offset` (clamped to the text, moved back to a
    /// character's start).
    fn position(&self, offset: u32) -> (u32, u32) {
        let mut offset = (offset as usize).min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset = offset.saturating_sub(1);
        }
        let line = self
            .starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let start = self.starts.get(line).copied().unwrap_or(0);
        let column = self
            .text
            .get(start..offset)
            .map_or(0, |part| part.encode_utf16().count());
        let narrow = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        (narrow(line), narrow(column))
    }
}

/// The on-disk copy of std file `name`, written to a per-version directory under the system's
/// temp directory when missing (the language server writes the same files).
fn std_file(name: &str) -> Option<PathBuf> {
    let text = kagari_std::file(name)?;
    let mut path = std::env::temp_dir().join(format!("kagari-std-{}", env!("CARGO_PKG_VERSION")));
    path.extend(name.split('/').filter(|part| !part.is_empty()));
    if !path.is_file() {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, text).ok()?;
    }
    Some(path)
}

/// A position: the file on disk, a 1-based line and a 1-based column (in UTF-16 units, as
/// editors count).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

/// What a source line can break at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineBreakpoints {
    /// The step IDs of the step forms on the line.
    pub steps: Vec<String>,
    /// Without steps: the first code word on the line of each story function.
    pub points: Vec<InstructionPoint>,
    /// Why the line cannot break, when it has neither.
    pub reason: Option<String>,
}

pub struct Locator {
    root: PathBuf,
    module: Option<Module>,
    sources: RefCell<HashMap<String, Option<Rc<Source>>>>,
}

impl Locator {
    /// Locations in the project at `root`; `bytecode` (a `game.kagari`) for line breakpoints.
    pub fn new(root: &Path, bytecode: Option<&Path>) -> Locator {
        let module = bytecode
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| kagari_bytecode::decode(&bytes).ok());
        Locator {
            root: root.to_path_buf(),
            module,
            sources: RefCell::new(HashMap::new()),
        }
    }

    fn source(&self, file: &str) -> Option<Rc<Source>> {
        if let Some(found) = self.sources.borrow().get(file) {
            return found.clone();
        }
        let on_disk = self.root.join(file);
        let path = if on_disk.is_file() {
            Some(on_disk)
        } else {
            file.strip_prefix("std/").and_then(std_file)
        };
        let source = path.and_then(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some(Rc::new(Source::new(path, text)))
        });
        self.sources
            .borrow_mut()
            .insert(file.to_owned(), source.clone());
        source
    }

    /// The line and column of `offset` in `file`.
    pub fn locate(&self, file: &str, offset: u32) -> Option<Location> {
        let source = self.source(file)?;
        let (line, column) = source.position(offset);
        Some(Location {
            path: source.path.clone(),
            line: line.saturating_add(1),
            column: column.saturating_add(1),
        })
    }

    /// `file:line:column`, with the file as the bytecode names it; `?` when unknown.
    pub fn describe(&self, file: Option<&str>, offset: Option<u32>) -> String {
        match (file, offset) {
            (Some(file), Some(offset)) => match self.locate(file, offset) {
                Some(at) => format!("{file}:{}:{}", at.line, at.column),
                None => format!("{file}@{offset}"),
            },
            (Some(file), None) => file.to_owned(),
            _ => "?".to_owned(),
        }
    }

    /// The 0-based line of `offset` in `file`.
    fn line_of(&self, file: &str, offset: u32) -> Option<u32> {
        Some(self.source(file)?.position(offset).0)
    }

    /// What line `line` (1-based) of the file at `path` can break at.
    pub fn breakpoints_at(&self, path: &Path, line: u32) -> LineBreakpoints {
        let Some(module) = &self.module else {
            return LineBreakpoints {
                reason: Some("no game.kagari: build the project first".to_owned()),
                ..LineBreakpoints::default()
            };
        };
        let Some(debug) = &module.debug else {
            return LineBreakpoints {
                reason: Some("the bytecode has no source positions".to_owned()),
                ..LineBreakpoints::default()
            };
        };
        let wanted = canonical(path);
        let target = line.saturating_sub(1);
        let string = |id: kagari_bytecode::StrId| {
            module.strings.get(id.0 as usize).map_or("", String::as_str)
        };
        let mut out = LineBreakpoints::default();
        let mut system_code = false;
        for (function, fn_debug) in module.functions.iter().zip(&debug.functions) {
            let Some(file) = fn_debug
                .file
                .and_then(|f| debug.files.get(f as usize))
                .map(|id| string(*id))
            else {
                continue;
            };
            let Some(source) = self.source(file) else {
                continue;
            };
            if canonical(&source.path) != wanted {
                continue;
            }
            // The source line of each position entry.
            let on_line: Vec<u32> = fn_debug
                .lines
                .iter()
                .filter(|(_, offset)| self.line_of(file, *offset) == Some(target))
                .map(|(word, _)| *word)
                .collect();
            if on_line.is_empty() {
                continue;
            }
            if function.kind == FnKind::Fn {
                system_code = true;
                continue;
            }
            let path = string(function.path);
            let words = word_offsets(&function.code);
            for (instr, op) in function.code.iter().enumerate() {
                let Instr::Step { id, .. } = op else {
                    continue;
                };
                let Some(&word) = words.get(instr) else {
                    continue;
                };
                let position = fn_debug
                    .lines
                    .iter()
                    .take_while(|(at, _)| *at <= word)
                    .last()
                    .map(|(_, offset)| *offset);
                if position.and_then(|offset| self.line_of(file, offset)) == Some(target) {
                    out.steps.push(format!("{path}#{id}"));
                }
            }
            if let Some(first) = on_line.iter().min() {
                out.points.push(InstructionPoint {
                    function: path.to_owned(),
                    word: *first,
                });
            }
        }
        if !out.steps.is_empty() {
            out.points.clear();
        } else if out.points.is_empty() {
            out.reason = Some(if system_code {
                "system code (`fn`) is not debugged".to_owned()
            } else {
                "no code on this line".to_owned()
            });
        }
        out
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

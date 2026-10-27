//! The Wadler/Prettier printer.
//!
//! - `Line` is a space when flat and a newline when broken; `SoftLine` is nothing when flat and a
//!   newline when broken; `HardLine` is always a newline.
//! - A newline requested at the start of a line (nothing written on it yet) writes nothing more;
//!   it only sets the indentation of the next text. Only `BlankLine` makes empty lines, and it
//!   makes exactly one.
//! - Indentation is written lazily before the first text of a line, and spaces before a newline
//!   are dropped, so lines never end in whitespace.
//! - Widths are `unicode-width` widths.

use unicode_width::UnicodeWidthStr;

use super::{Doc, breaks_inside, propagate_breaks};
use crate::{INDENT, MAX_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

#[derive(Clone, Copy)]
struct Cmd<'a> {
    indent: usize,
    mode: Mode,
    doc: &'a Doc,
}

/// Prints `doc`. The result ends with exactly one `\n`, or is empty if nothing was written.
pub(crate) fn print(doc: &mut Doc) -> String {
    propagate_breaks(doc);
    let mut printer = Printer::default();
    let mut stack = vec![Cmd {
        indent: 0,
        mode: Mode::Break,
        doc,
    }];
    while let Some(cmd) = stack.pop() {
        printer.step(cmd, &mut stack);
    }
    printer.finish()
}

#[derive(Default)]
struct Printer {
    out: String,
    /// Width of the current line so far.
    col: usize,
    /// Nothing written on the current line yet.
    line_started: bool,
    /// Indentation for the next text written at line start.
    pending_indent: usize,
    /// An empty line goes before the next text.
    blank: bool,
}

impl Printer {
    fn step<'a>(&mut self, cmd: Cmd<'a>, stack: &mut Vec<Cmd<'a>>) {
        let push = |stack: &mut Vec<Cmd<'a>>, indent: usize, mode: Mode, doc: &'a Doc| {
            stack.push(Cmd { indent, mode, doc });
        };
        match cmd.doc {
            Doc::Nil => {}
            Doc::Text(text) => self.text(text),
            Doc::Line => match cmd.mode {
                Mode::Flat => self.text(" "),
                Mode::Break => self.newline(cmd.indent),
            },
            Doc::SoftLine => {
                if cmd.mode == Mode::Break {
                    self.newline(cmd.indent);
                }
            }
            Doc::HardLine => self.newline(cmd.indent),
            Doc::BlankLine => {
                self.newline(cmd.indent);
                if !self.out.is_empty() {
                    self.blank = true;
                }
            }
            Doc::Concat(docs) => {
                for doc in docs.iter().rev() {
                    push(stack, cmd.indent, cmd.mode, doc);
                }
            }
            Doc::Indent(doc) => push(stack, cmd.indent.saturating_add(INDENT), cmd.mode, doc),
            Doc::Group { doc, broken } => {
                let flat = Cmd {
                    indent: cmd.indent,
                    mode: Mode::Flat,
                    doc,
                };
                let mode = if !*broken && fits(flat, stack, self.remaining()) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                push(stack, cmd.indent, mode, doc);
            }
            Doc::ConditionalGroup(states) => {
                let (mode, state) = self.choose(cmd.indent, states, stack);
                if let Some(state) = state {
                    push(stack, cmd.indent, mode, state);
                }
            }
            Doc::IfBreak { broken, flat } => {
                let doc = match cmd.mode {
                    Mode::Break => broken,
                    Mode::Flat => flat,
                };
                push(stack, cmd.indent, cmd.mode, doc);
            }
        }
    }

    /// The first state flat if it fits; else the first later state whose first line fits in
    /// break mode; else the last state in break mode.
    fn choose<'a>(
        &self,
        indent: usize,
        states: &'a [Doc],
        stack: &[Cmd<'a>],
    ) -> (Mode, Option<&'a Doc>) {
        let remaining = self.remaining();
        let try_state =
            |mode: Mode, doc: &'a Doc| fits(Cmd { indent, mode, doc }, stack, remaining);
        if let Some(first) = states.first()
            && !breaks_inside(first)
            && try_state(Mode::Flat, first)
        {
            return (Mode::Flat, Some(first));
        }
        let later = states.get(1..).unwrap_or_default();
        if let Some(state) = later.iter().find(|state| try_state(Mode::Break, state)) {
            return (Mode::Break, Some(state));
        }
        (Mode::Break, states.last())
    }

    /// Columns left on the current line; at line start the pending indentation counts.
    fn remaining(&self) -> usize {
        let col = if self.line_started {
            self.col
        } else {
            self.pending_indent
        };
        MAX_WIDTH.saturating_sub(col)
    }

    fn newline(&mut self, indent: usize) {
        if self.line_started {
            let trimmed = self.out.trim_end_matches([' ', '\t']).len();
            self.out.truncate(trimmed);
            self.out.push('\n');
            self.line_started = false;
            self.col = 0;
        }
        self.pending_indent = indent;
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if !self.line_started {
            if self.blank && !self.out.is_empty() {
                self.out.push('\n');
            }
            self.blank = false;
            self.out
                .extend(std::iter::repeat_n(' ', self.pending_indent));
            self.col = self.pending_indent;
            self.line_started = true;
        }
        self.out.push_str(text);
        match text.rsplit_once('\n') {
            Some((_, last)) => self.col = last.width(),
            None => self.col = self.col.saturating_add(text.width()),
        }
    }

    fn finish(mut self) -> String {
        let trimmed = self.out.trim_end().len();
        self.out.truncate(trimmed);
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        self.out
    }
}

/// Whether `next` fits in `width` columns, followed by the rest of the line from `rest` (the
/// printer's stack, top last) in each command's own mode, up to the first newline.
fn fits(next: Cmd<'_>, rest: &[Cmd<'_>], width: usize) -> bool {
    let mut remaining = width;
    let mut stack: Vec<(Mode, &Doc)> = vec![(next.mode, next.doc)];
    let mut rest = rest.iter().rev();
    loop {
        let (mode, doc) = match stack.pop() {
            Some(item) => item,
            None => match rest.next() {
                Some(cmd) => (cmd.mode, cmd.doc),
                None => return true,
            },
        };
        match doc {
            Doc::Nil => {}
            Doc::Text(text) => {
                let (first, ends_line) = match text.split_once('\n') {
                    Some((first, _)) => (first, true),
                    None => (text.as_str(), false),
                };
                match remaining.checked_sub(first.width()) {
                    Some(left) => remaining = left,
                    None => return false,
                }
                if ends_line {
                    return true;
                }
            }
            Doc::Line => match mode {
                Mode::Break => return true,
                Mode::Flat => match remaining.checked_sub(1) {
                    Some(left) => remaining = left,
                    None => return false,
                },
            },
            Doc::SoftLine => {
                if mode == Mode::Break {
                    return true;
                }
            }
            Doc::HardLine | Doc::BlankLine => return true,
            Doc::Concat(docs) => stack.extend(docs.iter().rev().map(|doc| (mode, doc))),
            Doc::Indent(doc) => stack.push((mode, doc)),
            Doc::Group { doc, broken } => {
                stack.push((if *broken { Mode::Break } else { mode }, doc));
            }
            Doc::ConditionalGroup(states) => {
                if let Some(first) = states.first() {
                    stack.push((mode, first));
                }
            }
            Doc::IfBreak { broken, flat } => stack.push((
                mode,
                match mode {
                    Mode::Break => broken,
                    Mode::Flat => flat,
                },
            )),
        }
    }
}

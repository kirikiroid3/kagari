//! Recursive-descent parser producing a lossless rowan tree.
//!
//! Trivia is attached lazily: pending whitespace and comments are emitted into the current node
//! right before the next significant token or node start, so nodes never begin or end with
//! trivia, and trailing trivia ends up in the root.
//!
//! The lexer emits single-character punctuation only. Composite punctuation (`::`, `>>=`, …) is
//! recognised here when its parts are adjacent, which keeps `>>` in `A<B<C>>` two tokens.

mod exprs;
mod forms;
mod items;
mod paths;
mod patterns;
mod types;

use rowan::{Checkpoint, GreenNode, GreenNodeBuilder, Language, TextRange, TextSize};

use super::{KagariLanguage, SyntaxKind};
use crate::SyntaxError;
use crate::lex::{lex, text_size};

use SyntaxKind::*;

/// Parses a whole file into a `SOURCE_FILE`. Forms are accepted in both source kinds; the
/// validator reports them in `.kis` files.
pub(crate) fn parse_source_file(text: &str) -> (GreenNode, Vec<SyntaxError>) {
    let mut p = Parser::new(text);
    p.builder.start_node(raw(SOURCE_FILE));
    p.items(false);
    p.finish_root()
}

fn raw(kind: SyntaxKind) -> rowan::SyntaxKind {
    KagariLanguage::kind_to_raw(kind)
}

pub(super) struct Parser<'t> {
    text: &'t str,
    tokens: Vec<(SyntaxKind, TextRange)>,
    /// Index of the next unconsumed token, trivia included.
    pos: usize,
    /// End of the last significant token consumed.
    last_end: TextSize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
}

impl<'t> Parser<'t> {
    fn new(text: &'t str) -> Parser<'t> {
        let (tokens, errors) = lex(text);
        Parser {
            text,
            tokens,
            pos: 0,
            last_end: TextSize::from(0),
            builder: GreenNodeBuilder::new(),
            errors,
        }
    }

    fn finish_root(mut self) -> (GreenNode, Vec<SyntaxError>) {
        // Anything left (only possible after a bug) is kept for losslessness.
        while self.pos < self.tokens.len() {
            self.bump_raw();
        }
        self.flush();
        self.builder.finish_node();
        (self.builder.finish(), self.errors)
    }

    // ---- token access -------------------------------------------------------------------

    /// Index into `tokens` of the `n`-th significant token from `pos`.
    fn nth_index(&self, n: usize) -> Option<usize> {
        let mut seen = 0usize;
        let mut i = self.pos;
        while let Some((kind, _)) = self.tokens.get(i) {
            if !kind.is_trivia() {
                if seen == n {
                    return Some(i);
                }
                seen = seen.saturating_add(1);
            }
            i = i.saturating_add(1);
        }
        None
    }

    /// The `n`-th significant token, counting single characters (a composite counts as its
    /// number of parts).
    fn nth(&self, n: usize) -> Option<SyntaxKind> {
        self.nth_index(n)
            .and_then(|i| self.tokens.get(i))
            .map(|(kind, _)| *kind)
    }

    fn current(&self) -> Option<SyntaxKind> {
        self.nth(0)
    }

    /// Whether the significant tokens from the `n`-th on spell `kind`. Composite kinds need
    /// their parts adjacent; single-character kinds match the single character only.
    fn nth_at(&self, n: usize, kind: SyntaxKind) -> bool {
        match kind.composite_parts() {
            Some(parts) => self.composite_at_index(self.nth_index(n), parts),
            None => self.nth(n) == Some(kind),
        }
    }

    fn composite_at_index(&self, start: Option<usize>, parts: &[SyntaxKind]) -> bool {
        let Some(start) = start else {
            return false;
        };
        parts.iter().enumerate().all(|(j, part)| {
            start
                .checked_add(j)
                .and_then(|i| self.tokens.get(i))
                .is_some_and(|(kind, _)| kind == part)
        })
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.nth_at(0, kind)
    }

    /// Whether the next token is an `IDENT` spelled `text`.
    fn at_contextual(&self, n: usize, text: &str) -> bool {
        self.nth(n) == Some(IDENT) && self.nth_text(n) == text
    }

    fn nth_range(&self, n: usize) -> TextRange {
        self.nth_index(n)
            .and_then(|i| self.tokens.get(i))
            .map(|(_, range)| *range)
            .unwrap_or_else(|| TextRange::empty(text_size(self.text.len())))
    }

    fn nth_text(&self, n: usize) -> &'t str {
        let range = self.nth_range(n);
        self.text
            .get(usize::from(range.start())..usize::from(range.end()))
            .unwrap_or_default()
    }

    fn current_range(&self) -> TextRange {
        self.nth_range(0)
    }

    // ---- tree building ------------------------------------------------------------------

    fn flush(&mut self) {
        while let Some(&(kind, range)) = self.tokens.get(self.pos) {
            if !kind.is_trivia() {
                break;
            }
            self.push_token(kind, range);
        }
    }

    fn push_token(&mut self, kind: SyntaxKind, range: TextRange) {
        let text = self
            .text
            .get(usize::from(range.start())..usize::from(range.end()))
            .unwrap_or_default();
        self.builder.token(raw(kind), text);
        self.pos = self.pos.saturating_add(1);
    }

    /// Consumes the next token as is, trivia included.
    fn bump_raw(&mut self) {
        if let Some(&(kind, range)) = self.tokens.get(self.pos) {
            self.push_token(kind, range);
            if !kind.is_trivia() {
                self.last_end = range.end();
            }
        }
    }

    /// Consumes the next significant token (after its leading trivia) with its lexed kind.
    fn bump(&mut self) {
        self.flush();
        self.bump_raw();
    }

    /// Consumes the next significant token as `kind` (contextual keywords).
    fn bump_remap(&mut self, kind: SyntaxKind) {
        self.flush();
        if let Some(&(_, range)) = self.tokens.get(self.pos) {
            self.push_token(kind, range);
            self.last_end = range.end();
        }
    }

    /// Consumes the parts of the composite `kind` as one token. Falls back to a plain bump if
    /// the parts are not there.
    fn bump_composite(&mut self, kind: SyntaxKind) {
        let Some(parts) = kind.composite_parts() else {
            self.bump();
            return;
        };
        if !self.at(kind) {
            self.bump();
            return;
        }
        self.flush();
        let first = self.tokens.get(self.pos).map(|(_, r)| *r);
        let last = self
            .pos
            .checked_add(parts.len().saturating_sub(1))
            .and_then(|i| self.tokens.get(i))
            .map(|(_, r)| *r);
        if let (Some(first), Some(last)) = (first, last) {
            let range = first.cover(last);
            let text = self
                .text
                .get(usize::from(range.start())..usize::from(range.end()))
                .unwrap_or_default();
            self.builder.token(raw(kind), text);
            self.pos = self.pos.saturating_add(parts.len());
            self.last_end = range.end();
        }
    }

    /// Consumes `kind` (composite or single) if it is next.
    fn eat(&mut self, kind: SyntaxKind) -> bool {
        if !self.at(kind) {
            return false;
        }
        if kind.composite_parts().is_some() {
            self.bump_composite(kind);
        } else {
            self.bump();
        }
        true
    }

    fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error_missing(format!("expected {}", kind.describe()));
            false
        }
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.flush();
        self.builder.start_node(raw(kind));
    }

    fn start_at(&mut self, checkpoint: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(checkpoint, raw(kind));
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }

    fn checkpoint(&mut self) -> Checkpoint {
        self.flush();
        self.builder.checkpoint()
    }

    // ---- errors -------------------------------------------------------------------------

    /// An error at the next significant token.
    fn error(&mut self, message: impl Into<String>) {
        let range = self.current_range();
        self.errors.push(SyntaxError::new(message, range));
    }

    /// An error for something missing right after the last consumed token.
    fn error_missing(&mut self, message: impl Into<String>) {
        self.errors
            .push(SyntaxError::new(message, TextRange::empty(self.last_end)));
    }

    /// Reports an error at the next token and wraps that token in an `ERROR` node.
    fn err_bump(&mut self, message: impl Into<String>) {
        self.error(message);
        self.start(ERROR);
        self.bump();
        self.finish();
    }

    /// Reports `message` and either wraps the next token in `ERROR` or, at a token some
    /// enclosing construct will want, leaves an empty `ERROR` node.
    fn err_missing_node(&mut self, message: &str, stop: &[SyntaxKind]) {
        self.error(message);
        self.start(ERROR);
        match self.current() {
            Some(kind) if !self.at_item_start() && !stop.contains(&kind) => self.bump(),
            _ => {}
        }
        self.finish();
    }

    /// After a list element: eats `,` and tells whether the list goes on. A missing comma is
    /// reported only when another element follows.
    fn list_comma(&mut self, next_element: impl Fn(&Self) -> bool) -> bool {
        if self.eat(COMMA) {
            return true;
        }
        if next_element(self) {
            self.error_missing("expected `,`");
            return true;
        }
        false
    }
}

//! Comment ownership.
//!
//! The parser attaches trivia before the next significant token, so ownership is computed per
//! token, independent of tree shape:
//!
//! - the *trailing* comments of a token are the comments after it before the first newline;
//! - the *leading* comments of a token are the other comments in the trivia run before it.
//!
//! Trivia after the last token belongs to a virtual end-of-file token.

use kagari_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, TextSize};
use rustc_hash::FxHashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Comment {
    /// Verbatim, except that line comments lose trailing whitespace.
    pub(crate) text: String,
    pub(crate) is_line: bool,
    /// A newline (or the start of the file) separates it from what comes before.
    pub(crate) newline_before: bool,
    /// Two or more newlines separate it from what comes before.
    pub(crate) blank_line_before: bool,
    /// A newline follows it directly (only whitespace in between).
    pub(crate) newline_after: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TokenTrivia {
    pub(crate) leading: Vec<Comment>,
    pub(crate) trailing: Vec<Comment>,
    /// The source has an empty line before the token (or before its first leading comment).
    pub(crate) blank_before: bool,
    /// The source has an empty line between the last leading comment and the token.
    pub(crate) blank_after_leading: bool,
}

pub(crate) struct Trivia {
    tokens: FxHashMap<TextSize, TokenTrivia>,
    eof: TokenTrivia,
}

const NONE: &TokenTrivia = &TokenTrivia {
    leading: Vec::new(),
    trailing: Vec::new(),
    blank_before: false,
    blank_after_leading: false,
};

impl Trivia {
    pub(crate) fn new(root: &SyntaxNode) -> Trivia {
        let mut trivia = Trivia {
            tokens: FxHashMap::default(),
            eof: TokenTrivia::default(),
        };
        let mut run: Vec<SyntaxToken> = Vec::new();
        let mut prev: Option<TextSize> = None;
        for token in root
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
        {
            if token.kind().is_trivia() {
                run.push(token);
            } else {
                let start = token.text_range().start();
                trivia.split_run(&run, prev, Some(start));
                run.clear();
                prev = Some(start);
            }
        }
        trivia.split_run(&run, prev, None);
        trivia
    }

    /// The comments around the significant token `token`.
    pub(crate) fn of(&self, token: &SyntaxToken) -> &TokenTrivia {
        self.tokens.get(&token.text_range().start()).unwrap_or(NONE)
    }

    pub(crate) fn eof(&self) -> &TokenTrivia {
        &self.eof
    }

    fn split_run(&mut self, run: &[SyntaxToken], prev: Option<TextSize>, next: Option<TextSize>) {
        let mut trailing = Vec::new();
        let mut leading: Vec<Comment> = Vec::new();
        let mut newlines = 0usize;
        let mut seen_newline = false;
        let mut after_comment = false;
        for token in run {
            let text = token.text();
            if token.kind() == SyntaxKind::WHITESPACE {
                let count = text.matches('\n').count();
                if count > 0 {
                    seen_newline = true;
                    if after_comment && let Some(last) = leading.last_mut() {
                        last.newline_after = true;
                    }
                }
                newlines = newlines.saturating_add(count);
                after_comment = false;
                continue;
            }
            let is_line = token.kind() == SyntaxKind::LINE_COMMENT;
            let text = if is_line { text.trim_end() } else { text };
            if !seen_newline && prev.is_some() {
                trailing.push(Comment {
                    text: text.to_owned(),
                    is_line,
                    newline_before: false,
                    blank_line_before: false,
                    newline_after: false,
                });
                after_comment = false;
            } else {
                leading.push(Comment {
                    text: text.to_owned(),
                    is_line,
                    newline_before: newlines > 0 || prev.is_none(),
                    blank_line_before: newlines >= 2,
                    newline_after: false,
                });
                after_comment = true;
            }
            newlines = 0;
        }
        let blank_before = match leading.first() {
            Some(first) => first.blank_line_before,
            None => newlines >= 2,
        };
        let blank_after_leading = !leading.is_empty() && newlines >= 2;
        if let Some(prev) = prev
            && !trailing.is_empty()
        {
            self.tokens.entry(prev).or_default().trailing = trailing;
        }
        let entry = match next {
            Some(next) => self.tokens.entry(next).or_default(),
            None => &mut self.eof,
        };
        entry.leading = leading;
        entry.blank_before = blank_before;
        entry.blank_after_leading = blank_after_leading;
    }
}

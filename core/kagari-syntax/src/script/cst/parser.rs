//! Builds the lossless `.kis` CST from the token stream.

use rowan::GreenNodeBuilder;

use crate::script::ast::text::{Piece, decode_reference, split_references};
use crate::script::lex::lex_with_errors;
use crate::{Parse, SyntaxError, TextRange, TextSize};

use super::{ScriptKind, ScriptLanguage};

/// Parses a `.kis` document. The tree text always equals `text`.
pub fn parse(text: &str) -> Parse<ScriptLanguage> {
    let (lexed, errors) = lex_with_errors(text);
    let mut tokens = Vec::with_capacity(lexed.len());
    let mut offset = TextSize::new(0);
    for (kind, len) in lexed {
        let end = offset.checked_add(len).unwrap_or(offset);
        tokens.push(Token {
            kind,
            range: TextRange::new(offset, end),
        });
        offset = end;
    }
    let mut parser = Parser {
        text,
        tokens,
        pos: 0,
        builder: GreenNodeBuilder::new(),
        errors,
        open: Vec::new(),
    };
    parser.document();
    let mut errors = parser.errors;
    errors.sort_by_key(|error| (error.range.start(), error.range.end()));
    Parse::new(parser.builder.finish(), errors)
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: ScriptKind,
    range: TextRange,
}

/// An element whose start tag was parsed and whose `ELEMENT` node is still open.
struct Open {
    name: Option<String>,
    name_range: TextRange,
}

struct Parser<'a> {
    text: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
    open: Vec<Open>,
}

impl<'a> Parser<'a> {
    fn nth(&self, n: usize) -> Option<Token> {
        self.tokens.get(self.pos.saturating_add(n)).copied()
    }

    fn current(&self) -> Option<ScriptKind> {
        self.nth(0).map(|token| token.kind)
    }

    fn token_text(&self, token: Token) -> &'a str {
        let start = usize::from(token.range.start());
        let end = usize::from(token.range.end());
        self.text.get(start..end).unwrap_or("")
    }

    /// Start of the current token, or the end of input.
    fn here(&self) -> TextSize {
        self.nth(0)
            .map_or_else(|| TextSize::of(self.text), |token| token.range.start())
    }

    /// End of the previously consumed token.
    fn prev_end(&self) -> TextSize {
        self.pos
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or(TextSize::new(0), |token| token.range.end())
    }

    fn error(&mut self, message: impl Into<String>, range: TextRange) {
        self.errors.push(SyntaxError::new(message, range));
    }

    /// Adds the current token to the tree, validating references it contains.
    fn bump(&mut self) {
        let Some(token) = self.nth(0) else {
            return;
        };
        match token.kind {
            ScriptKind::ENTITY_REF => self.check_entity_ref(token),
            ScriptKind::ATTR_VALUE => self.check_attr_value(token),
            _ => {}
        }
        let text = self.token_text(token);
        self.builder.token(token.kind.into(), text);
        self.pos = self.pos.saturating_add(1);
    }

    /// Wraps the current token in an `ERROR` node.
    fn bump_error(&mut self, message: Option<&str>) {
        if let (Some(message), Some(token)) = (message, self.nth(0)) {
            self.error(message, token.range);
        }
        self.builder.start_node(ScriptKind::ERROR.into());
        self.bump();
        self.builder.finish_node();
    }

    fn check_entity_ref(&mut self, token: Token) {
        let raw = self.token_text(token);
        let body = raw
            .strip_prefix('&')
            .and_then(|rest| rest.strip_suffix(';'))
            .unwrap_or("");
        if let Err(error) = decode_reference(body) {
            let message = error.message(raw);
            self.error(message, token.range);
        }
    }

    fn check_attr_value(&mut self, token: Token) {
        let raw = self.token_text(token);
        let mut chars = raw.chars();
        let Some(quote) = chars.next() else {
            return;
        };
        let inner = raw.get(1..).unwrap_or("");
        let terminated = inner.ends_with(quote);
        let inner = inner.strip_suffix(quote).unwrap_or(inner);
        let inner_start = token
            .range
            .start()
            .checked_add(TextSize::of(quote))
            .unwrap_or(token.range.start());
        let at = |offset: usize, len: usize| -> TextRange {
            let start = inner_start.checked_add(size(offset)).unwrap_or(inner_start);
            let end = start.checked_add(size(len)).unwrap_or(start);
            TextRange::new(start, end)
        };
        let mut found = Vec::new();
        for piece in split_references(inner) {
            match piece {
                Piece::Text(range) => {
                    if terminated {
                        let text = inner.get(range.clone()).unwrap_or("");
                        for (index, _) in text.match_indices('<') {
                            found.push((
                                "`<` must be escaped as `&lt;` in attribute values".to_owned(),
                                at(range.start.saturating_add(index), 1),
                            ));
                        }
                    }
                }
                Piece::BareAmp(offset) => {
                    found.push(("`&` must be escaped as `&amp;`".to_owned(), at(offset, 1)));
                }
                Piece::Ref { range, decoded } => {
                    if let Err(error) = decoded {
                        let raw_ref = inner.get(range.clone()).unwrap_or("");
                        found.push((
                            error.message(raw_ref),
                            at(range.start, range.end.saturating_sub(range.start)),
                        ));
                    }
                }
            }
        }
        for (message, range) in found {
            self.error(message, range);
        }
    }

    fn skip_whitespace(&mut self) {
        while self.current() == Some(ScriptKind::WHITESPACE) {
            self.bump();
        }
    }

    /// Kind of the first token from the current one that is not tag whitespace.
    fn peek_past_whitespace(&self) -> Option<ScriptKind> {
        let mut n = 0usize;
        loop {
            match self.nth(n).map(|token| token.kind) {
                Some(ScriptKind::WHITESPACE) => n = n.saturating_add(1),
                other => return other,
            }
        }
    }

    fn document(&mut self) {
        self.builder.start_node(ScriptKind::DOCUMENT.into());
        let mut seen_root = false;
        while let Some(token) = self.nth(0) {
            match token.kind {
                ScriptKind::L_ANGLE if seen_root => {
                    let start = token.range.start();
                    self.builder.start_node(ScriptKind::ERROR.into());
                    self.element();
                    self.builder.finish_node();
                    let range = TextRange::new(start, self.prev_end().max(start));
                    self.error("a document must have exactly one root element", range);
                }
                ScriptKind::L_ANGLE => {
                    seen_root = true;
                    self.element();
                }
                ScriptKind::L_ANGLE_SLASH => {
                    let start = token.range.start();
                    self.builder.start_node(ScriptKind::ERROR.into());
                    self.end_tag();
                    self.builder.finish_node();
                    let range = TextRange::new(start, self.prev_end().max(start));
                    self.error("unexpected end tag", range);
                }
                ScriptKind::TEXT if self.token_text(token).trim().is_empty() => self.bump(),
                ScriptKind::COMMENT => self.bump(),
                ScriptKind::TEXT | ScriptKind::ENTITY_REF | ScriptKind::CDATA => {
                    self.bump_error(Some("text is not allowed outside the root element"));
                }
                _ => self.misc_markup(token),
            }
        }
        if !seen_root {
            let end = TextSize::of(self.text);
            self.error("missing root element", TextRange::empty(end));
        }
        self.builder.finish_node();
    }

    /// Markup that is an error wherever it appears: misplaced XML declarations, processing
    /// instructions, DOCTYPE, lexer error tokens and stray tag tokens.
    fn misc_markup(&mut self, token: Token) {
        match token.kind {
            ScriptKind::XML_DECL if token.range.start() == TextSize::new(0) => self.bump(),
            ScriptKind::XML_DECL => {
                self.bump_error(Some("XML declaration must be at the start"));
            }
            ScriptKind::PI => {
                self.bump_error(Some("processing instructions are not supported"));
            }
            ScriptKind::DOCTYPE => {
                self.bump_error(Some("DOCTYPE declarations are not supported"));
            }
            // The lexer already reported it.
            ScriptKind::ERROR_TOKEN => self.bump_error(None),
            _ => self.bump_error(Some("unexpected token")),
        }
    }

    /// Parses an element starting at `<`, including all of its descendants.
    fn element(&mut self) {
        let base = self.open.len();
        self.open_element();
        while self.open.len() > base {
            let Some(token) = self.nth(0) else {
                while self.open.len() > base {
                    self.close_unclosed();
                }
                break;
            };
            match token.kind {
                ScriptKind::L_ANGLE => self.open_element(),
                ScriptKind::L_ANGLE_SLASH => {
                    let name = self
                        .nth(1)
                        .filter(|next| next.kind == ScriptKind::NAME)
                        .map(|next| self.token_text(next).to_owned());
                    let matching = name.as_deref().and_then(|name| {
                        self.open
                            .iter()
                            .enumerate()
                            .skip(base)
                            .rev()
                            .find(|(_, open)| open.name.as_deref() == Some(name))
                            .map(|(index, _)| index)
                    });
                    match matching {
                        Some(index) if index.saturating_add(1) == self.open.len() => {
                            self.end_tag();
                            self.open.pop();
                            self.builder.finish_node();
                        }
                        Some(index) => {
                            while self.open.len() > index.saturating_add(1) {
                                self.close_unclosed();
                            }
                        }
                        None => {
                            let start = token.range.start();
                            self.builder.start_node(ScriptKind::ERROR.into());
                            self.end_tag();
                            self.builder.finish_node();
                            let range = TextRange::new(start, self.prev_end().max(start));
                            let message = match &name {
                                Some(name) => format!("unexpected end tag `</{name}>`"),
                                None => "unexpected end tag".to_owned(),
                            };
                            self.error(message, range);
                        }
                    }
                }
                ScriptKind::TEXT
                | ScriptKind::ENTITY_REF
                | ScriptKind::CDATA
                | ScriptKind::COMMENT => self.bump(),
                _ => self.misc_markup(token),
            }
        }
    }

    /// Closes the innermost open element, which has no end tag.
    fn close_unclosed(&mut self) {
        if let Some(open) = self.open.pop() {
            let message = match &open.name {
                Some(name) => format!("unclosed element `<{name}>`"),
                None => "unclosed element".to_owned(),
            };
            self.error(message, open.name_range);
            self.builder.finish_node();
        }
    }

    /// Starts an `ELEMENT` at `<` and parses its tag. An empty-element tag finishes the element;
    /// otherwise it is pushed onto the open stack.
    fn open_element(&mut self) {
        let start = self.here();
        self.builder.start_node(ScriptKind::ELEMENT.into());
        let tag = self.builder.checkpoint();
        self.bump(); // `<`
        let mut open = Open {
            name: None,
            name_range: TextRange::empty(start),
        };
        if let Some(token) = self.nth(0).filter(|token| token.kind == ScriptKind::NAME) {
            self.check_name(token);
            open.name = Some(self.token_text(token).to_owned());
            open.name_range = token.range;
            self.bump();
        } else {
            self.error("expected element name", TextRange::empty(self.here()));
        }
        let mut seen: Vec<String> = Vec::new();
        let kind = loop {
            let Some(token) = self.nth(0) else {
                self.error("expected `>`", TextRange::empty(self.here()));
                break ScriptKind::START_TAG;
            };
            match token.kind {
                ScriptKind::WHITESPACE => self.bump(),
                ScriptKind::NAME => self.attribute(&mut seen),
                ScriptKind::R_ANGLE => {
                    self.bump();
                    break ScriptKind::START_TAG;
                }
                ScriptKind::SLASH_R_ANGLE => {
                    self.bump();
                    break ScriptKind::EMPTY_ELEM_TAG;
                }
                ScriptKind::ATTR_VALUE => self.bump_error(Some("expected attribute name")),
                ScriptKind::EQ => self.bump_error(Some("expected attribute name")),
                // The lexer already reported it.
                ScriptKind::ERROR_TOKEN => self.bump_error(None),
                _ => {
                    self.error("expected `>`", TextRange::empty(self.prev_end()));
                    break ScriptKind::START_TAG;
                }
            }
        };
        self.builder.start_node_at(tag, kind.into());
        self.builder.finish_node();
        if kind == ScriptKind::EMPTY_ELEM_TAG {
            self.builder.finish_node();
        } else {
            self.open.push(open);
        }
    }

    /// `NAME (WS* '=' WS* ATTR_VALUE)?`. Whitespace is only taken into the attribute when an
    /// `=` follows the name.
    fn attribute(&mut self, seen: &mut Vec<String>) {
        let Some(name) = self.nth(0) else {
            return;
        };
        self.builder.start_node(ScriptKind::ATTRIBUTE.into());
        self.check_name(name);
        let name_text = self.token_text(name).to_owned();
        if seen.contains(&name_text) {
            self.error(format!("duplicate attribute `{name_text}`"), name.range);
        } else {
            seen.push(name_text.clone());
        }
        self.bump();
        if self.peek_past_whitespace() == Some(ScriptKind::EQ) {
            self.skip_whitespace();
            self.bump(); // `=`
            if self.peek_past_whitespace() == Some(ScriptKind::ATTR_VALUE) {
                self.skip_whitespace();
                self.bump();
            } else {
                self.error(
                    format!("expected a value for attribute `{name_text}`"),
                    TextRange::empty(self.prev_end()),
                );
            }
        } else {
            self.error(format!("attribute `{name_text}` has no value"), name.range);
        }
        self.builder.finish_node();
    }

    /// `</ NAME WS* >`, consuming any other tag tokens up to the closing `>`.
    fn end_tag(&mut self) {
        self.builder.start_node(ScriptKind::END_TAG.into());
        self.bump(); // `</`
        match self.nth(0) {
            Some(token) if token.kind == ScriptKind::NAME => {
                self.check_name(token);
                self.bump();
            }
            _ => self.error("expected element name", TextRange::empty(self.here())),
        }
        self.skip_whitespace();
        let mut junk: Option<TextRange> = None;
        loop {
            match self.nth(0) {
                Some(token) if token.kind == ScriptKind::R_ANGLE => {
                    self.bump();
                    break;
                }
                Some(token) if token.kind == ScriptKind::SLASH_R_ANGLE => {
                    self.error("expected `>`", token.range);
                    self.bump();
                    break;
                }
                Some(token)
                    if matches!(
                        token.kind,
                        ScriptKind::NAME
                            | ScriptKind::EQ
                            | ScriptKind::ATTR_VALUE
                            | ScriptKind::WHITESPACE
                            | ScriptKind::ERROR_TOKEN
                    ) =>
                {
                    if token.kind != ScriptKind::WHITESPACE {
                        junk = Some(junk.map_or(token.range, |range| range.cover(token.range)));
                    }
                    self.bump();
                }
                _ => {
                    self.error("expected `>`", TextRange::empty(self.prev_end()));
                    break;
                }
            }
        }
        if let Some(range) = junk {
            self.error("end tags cannot have attributes", range);
        }
        self.builder.finish_node();
    }

    fn check_name(&mut self, token: Token) {
        if self.token_text(token).contains(':') {
            self.error("namespace prefixes are not supported", token.range);
        }
    }
}

fn size(len: usize) -> TextSize {
    TextSize::new(u32::try_from(len).unwrap_or(u32::MAX))
}

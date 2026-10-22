//! Lexer for `.kis`/`.kix`.
//!
//! Punctuation is emitted one character per token; the parser glues adjacent characters into
//! composite tokens (`::`, `>>=`, …) where the grammar wants them.

use logos::Logos;
use rowan::{TextRange, TextSize};

use crate::SyntaxError;
use crate::cst::SyntaxKind;

/// Whether a block comment or string found its closing delimiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Closed {
    Yes,
    No,
}

#[derive(Logos, Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    #[regex(r"[ \t\r\n\u{000B}\u{000C}\u{0085}\u{200E}\u{200F}\u{2028}\u{2029}]+")]
    Whitespace,
    #[regex(r"//[^\n]*", allow_greedy = true)]
    LineComment,
    #[token("/*", block_comment)]
    BlockComment(Closed),
    #[regex(r"[\p{XID_Start}_]\p{XID_Continue}*")]
    Ident,
    #[regex(r"[0-9][0-9a-zA-Z_]*")]
    Int,
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*")]
    Decimal,
    #[token("\"", string)]
    Str(Closed),
    #[regex(r"'[\p{XID_Start}_]\p{XID_Continue}*")]
    Label,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LCurly,
    #[token("}")]
    RCurly,
    #[token("[")]
    LBrack,
    #[token("]")]
    RBrack,
    #[token("<")]
    LAngle,
    #[token(">")]
    RAngle,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token(".")]
    Dot,
    #[token("=")]
    Eq,
    #[token("!")]
    Bang,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("#")]
    Pound,
    #[token("?")]
    Question,
}

/// Consumes a (nested) block comment after its opening `/*`.
fn block_comment(lex: &mut logos::Lexer<'_, Tok>) -> Closed {
    let rest = lex.remainder().as_bytes();
    let mut depth = 1usize;
    let mut i = 0usize;
    while let Some(&byte) = rest.get(i) {
        let next = rest.get(i.saturating_add(1)).copied();
        match (byte, next) {
            (b'/', Some(b'*')) => {
                depth = depth.saturating_add(1);
                i = i.saturating_add(2);
            }
            (b'*', Some(b'/')) => {
                depth = depth.saturating_sub(1);
                i = i.saturating_add(2);
                if depth == 0 {
                    lex.bump(i);
                    return Closed::Yes;
                }
            }
            _ => i = i.saturating_add(1),
        }
    }
    lex.bump(rest.len());
    Closed::No
}

/// Consumes a string literal after its opening `"`, up to the closing quote or EOF.
/// Escapes are validated later.
fn string(lex: &mut logos::Lexer<'_, Tok>) -> Closed {
    let rest = lex.remainder().as_bytes();
    let mut i = 0usize;
    while let Some(&byte) = rest.get(i) {
        match byte {
            b'"' => {
                lex.bump(i.saturating_add(1));
                return Closed::Yes;
            }
            // Skipping one byte is enough: we only look for ASCII `"` and `\`, and UTF-8
            // continuation bytes never match them.
            b'\\' => i = i.saturating_add(2),
            _ => i = i.saturating_add(1),
        }
    }
    lex.bump(rest.len());
    Closed::No
}

fn kind_of(tok: Tok, text: &str) -> SyntaxKind {
    use SyntaxKind as K;
    match tok {
        Tok::Whitespace => K::WHITESPACE,
        Tok::LineComment => K::LINE_COMMENT,
        Tok::BlockComment(_) => K::BLOCK_COMMENT,
        Tok::Ident if text == "_" => K::UNDERSCORE,
        Tok::Ident => K::from_keyword(text).unwrap_or(K::IDENT),
        Tok::Int => K::INT_NUMBER,
        Tok::Decimal => K::DECIMAL_NUMBER,
        Tok::Str(_) => K::STRING,
        Tok::Label => K::LABEL,
        Tok::LParen => K::L_PAREN,
        Tok::RParen => K::R_PAREN,
        Tok::LCurly => K::L_CURLY,
        Tok::RCurly => K::R_CURLY,
        Tok::LBrack => K::L_BRACK,
        Tok::RBrack => K::R_BRACK,
        Tok::LAngle => K::L_ANGLE,
        Tok::RAngle => K::R_ANGLE,
        Tok::Comma => K::COMMA,
        Tok::Semicolon => K::SEMICOLON,
        Tok::Colon => K::COLON,
        Tok::Dot => K::DOT,
        Tok::Eq => K::EQ,
        Tok::Bang => K::BANG,
        Tok::Plus => K::PLUS,
        Tok::Minus => K::MINUS,
        Tok::Star => K::STAR,
        Tok::Slash => K::SLASH,
        Tok::Percent => K::PERCENT,
        Tok::Amp => K::AMP,
        Tok::Pipe => K::PIPE,
        Tok::Caret => K::CARET,
        Tok::Pound => K::POUND,
        Tok::Question => K::QUESTION,
    }
}

pub(crate) fn text_size(offset: usize) -> TextSize {
    TextSize::from(u32::try_from(offset).unwrap_or(u32::MAX))
}

/// Splits `text` into tokens covering it exactly. Adjacent unrecognised characters are merged
/// into one `ERROR_TOKEN`. Errors for unterminated comments and strings and for unknown
/// characters are returned separately, sorted by position.
pub fn lex(text: &str) -> (Vec<(SyntaxKind, TextRange)>, Vec<SyntaxError>) {
    let mut tokens: Vec<(SyntaxKind, TextRange)> = Vec::new();
    let mut errors = Vec::new();
    let mut lexer = Tok::lexer(text);
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        let range = TextRange::new(text_size(span.start), text_size(span.end));
        match result {
            Ok(tok) => {
                match tok {
                    Tok::BlockComment(Closed::No) => {
                        errors.push(SyntaxError::new("unterminated block comment", range));
                    }
                    Tok::Str(Closed::No) => {
                        errors.push(SyntaxError::new("unterminated string literal", range));
                    }
                    _ => {}
                }
                tokens.push((kind_of(tok, lexer.slice()), range));
            }
            Err(()) => match tokens.last_mut() {
                Some((SyntaxKind::ERROR_TOKEN, last)) if last.end() == range.start() => {
                    *last = last.cover(range);
                }
                _ => tokens.push((SyntaxKind::ERROR_TOKEN, range)),
            },
        }
    }
    for (kind, range) in &tokens {
        if *kind == SyntaxKind::ERROR_TOKEN {
            errors.push(SyntaxError::new("unexpected character", *range));
        }
    }
    errors.sort_by_key(|error| error.range.start());
    (split_tuple_indices(text, tokens), errors)
}

/// Splits a `DECIMAL_NUMBER` directly after a `.` into `INT_NUMBER . INT_NUMBER`, so that
/// `t.0.1` is two tuple-field accesses. A `..` before the number (`0..1.5`) keeps it whole.
fn split_tuple_indices(
    text: &str,
    tokens: Vec<(SyntaxKind, TextRange)>,
) -> Vec<(SyntaxKind, TextRange)> {
    let mut out: Vec<(SyntaxKind, TextRange)> = Vec::with_capacity(tokens.len());
    for (kind, range) in tokens {
        let after_dot = out.last().map(|(k, _)| *k) == Some(SyntaxKind::DOT);
        let after_dot2 = after_dot
            && out
                .len()
                .checked_sub(2)
                .and_then(|i| out.get(i))
                .map(|(k, _)| *k)
                == Some(SyntaxKind::DOT);
        let dot = text
            .get(usize::from(range.start())..usize::from(range.end()))
            .and_then(|t| t.find('.'));
        match dot {
            Some(dot) if kind == SyntaxKind::DECIMAL_NUMBER && after_dot && !after_dot2 => {
                let dot_start = range.start().checked_add(text_size(dot));
                let dot_end = dot_start.and_then(|s| s.checked_add(TextSize::from(1)));
                if let (Some(dot_start), Some(dot_end)) = (dot_start, dot_end) {
                    out.push((
                        SyntaxKind::INT_NUMBER,
                        TextRange::new(range.start(), dot_start),
                    ));
                    out.push((SyntaxKind::DOT, TextRange::new(dot_start, dot_end)));
                    out.push((SyntaxKind::INT_NUMBER, TextRange::new(dot_end, range.end())));
                } else {
                    out.push((kind, range));
                }
            }
            _ => out.push((kind, range)),
        }
    }
    out
}

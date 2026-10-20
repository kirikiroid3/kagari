//! Lexer for `.kirp`/`.kix`.

use logos::Logos;
use rowan::{TextRange, TextSize};

use crate::SyntaxError;
use crate::represent::cst::RepresentKind;

/// Whether a block comment found its closing `*/`.
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
    #[regex(r"[0-9][0-9_]*")]
    Int,
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*")]
    Decimal,
    #[token("\"", string)]
    Str,
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
    #[token("::")]
    Colon2,
    #[token(".")]
    Dot,
    #[token("..")]
    Dot2,
    #[token("..=")]
    Dot2Eq,
    #[token("->")]
    ThinArrow,
    #[token("=>")]
    FatArrow,
    #[token("=")]
    Eq,
    #[token("==")]
    Eq2,
    #[token("!=")]
    Neq,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
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
    #[token("&&")]
    Amp2,
    #[token("|")]
    Pipe,
    #[token("||")]
    Pipe2,
    #[token("!")]
    Bang,
    #[token("#")]
    Pound,
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
/// Escapes are validated later by the parser.
fn string(lex: &mut logos::Lexer<'_, Tok>) {
    let rest = lex.remainder().as_bytes();
    let mut i = 0usize;
    let mut end = rest.len();
    while let Some(&byte) = rest.get(i) {
        match byte {
            b'"' => {
                end = i.saturating_add(1);
                break;
            }
            // Skipping one byte is enough: we only look for ASCII `"` and `\`, and UTF-8
            // continuation bytes never match them.
            b'\\' => i = i.saturating_add(2),
            _ => i = i.saturating_add(1),
        }
    }
    lex.bump(end.min(rest.len()));
}

fn kind_of(tok: Tok, text: &str) -> RepresentKind {
    use RepresentKind as K;
    match tok {
        Tok::Whitespace => K::WHITESPACE,
        Tok::LineComment => K::LINE_COMMENT,
        Tok::BlockComment(_) => K::BLOCK_COMMENT,
        Tok::Ident if text == "_" => K::UNDERSCORE,
        Tok::Ident => K::from_keyword(text).unwrap_or(K::IDENT),
        Tok::Int => K::INT_NUMBER,
        Tok::Decimal => K::DECIMAL_NUMBER,
        Tok::Str => K::STRING,
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
        Tok::Colon2 => K::COLON2,
        Tok::Dot => K::DOT,
        Tok::Dot2 => K::DOT2,
        Tok::Dot2Eq => K::DOT2EQ,
        Tok::ThinArrow => K::THIN_ARROW,
        Tok::FatArrow => K::FAT_ARROW,
        Tok::Eq => K::EQ,
        Tok::Eq2 => K::EQ2,
        Tok::Neq => K::NEQ,
        Tok::LtEq => K::LTEQ,
        Tok::GtEq => K::GTEQ,
        Tok::Plus => K::PLUS,
        Tok::Minus => K::MINUS,
        Tok::Star => K::STAR,
        Tok::Slash => K::SLASH,
        Tok::Percent => K::PERCENT,
        Tok::Amp => K::AMP,
        Tok::Amp2 => K::AMP2,
        Tok::Pipe => K::PIPE,
        Tok::Pipe2 => K::PIPE2,
        Tok::Bang => K::BANG,
        Tok::Pound => K::POUND,
    }
}

pub(crate) fn text_size(offset: usize) -> TextSize {
    TextSize::from(u32::try_from(offset).unwrap_or(u32::MAX))
}

/// Splits `text` into tokens covering it exactly. Adjacent unrecognised characters are merged
/// into one `ERROR_TOKEN`. Errors for unterminated block comments and unknown characters are
/// returned separately.
pub fn lex(text: &str) -> (Vec<(RepresentKind, TextRange)>, Vec<SyntaxError>) {
    let mut tokens: Vec<(RepresentKind, TextRange)> = Vec::new();
    let mut errors = Vec::new();
    let mut lexer = Tok::lexer(text);
    while let Some(result) = lexer.next() {
        let span = lexer.span();
        let range = TextRange::new(text_size(span.start), text_size(span.end));
        match result {
            Ok(tok) => {
                if tok == Tok::BlockComment(Closed::No) {
                    errors.push(SyntaxError::new("unterminated block comment", range));
                }
                tokens.push((kind_of(tok, lexer.slice()), range));
            }
            Err(()) => match tokens.last_mut() {
                Some((RepresentKind::ERROR_TOKEN, last)) if last.end() == range.start() => {
                    *last = last.cover(range);
                }
                _ => tokens.push((RepresentKind::ERROR_TOKEN, range)),
            },
        }
    }
    for (kind, range) in &tokens {
        if *kind == RepresentKind::ERROR_TOKEN {
            errors.push(SyntaxError::new("unexpected character", *range));
        }
    }
    errors.sort_by_key(|error| error.range.start());
    (tokens, errors)
}

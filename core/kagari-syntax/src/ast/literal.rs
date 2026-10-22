//! Literals, string unescaping and integer literal parsing.

use rowan::TextRange;

use super::first_token;
use crate::cst::{SyntaxKind, SyntaxToken};
use crate::lex::text_size;

ast_node!(Literal, LITERAL);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LiteralKind {
    Int,
    Decimal,
    Str,
    Bool(bool),
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        first_token(&self.0)
    }

    pub fn kind(&self) -> Option<LiteralKind> {
        Some(match self.token()?.kind() {
            SyntaxKind::INT_NUMBER => LiteralKind::Int,
            SyntaxKind::DECIMAL_NUMBER => LiteralKind::Decimal,
            SyntaxKind::STRING => LiteralKind::Str,
            SyntaxKind::TRUE_KW => LiteralKind::Bool(true),
            SyntaxKind::FALSE_KW => LiteralKind::Bool(false),
            _ => return None,
        })
    }

    /// The value and suffix of a valid integer literal (`0x1Fu8` → `(31, Some("u8"))`).
    pub fn int_value(&self) -> Option<(u128, Option<String>)> {
        let token = self.token()?;
        if token.kind() != SyntaxKind::INT_NUMBER {
            return None;
        }
        let (value, suffix) = parse_int_literal(token.text()).ok()?;
        Some((value, suffix.map(str::to_owned)))
    }

    /// The source text of a decimal literal (`1.5`).
    pub fn decimal_text(&self) -> Option<String> {
        let token = self.token()?;
        (token.kind() == SyntaxKind::DECIMAL_NUMBER).then(|| token.text().to_owned())
    }

    /// The unescaped value of a string literal.
    pub fn string_value(&self) -> Option<String> {
        let token = self.token()?;
        if token.kind() != SyntaxKind::STRING {
            return None;
        }
        unescape_string(token.text()).ok()
    }
}

/// The suffixes an integer literal may carry.
pub const INT_SUFFIXES: [&str; 12] = [
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// Why an integer literal is invalid.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum IntLiteralError {
    NoDigits,
    InvalidDigit { digit: char, radix: u32 },
    InvalidSuffix(String),
    TooLarge,
}

impl IntLiteralError {
    pub fn message(&self) -> String {
        match self {
            IntLiteralError::NoDigits => "integer literal has no digits".to_owned(),
            IntLiteralError::InvalidDigit { digit, radix } => {
                let base = match radix {
                    2 => "binary",
                    8 => "octal",
                    16 => "hexadecimal",
                    _ => "decimal",
                };
                format!("invalid digit `{digit}` in a {base} literal")
            }
            IntLiteralError::InvalidSuffix(suffix) => {
                format!("invalid suffix `{suffix}` for an integer literal")
            }
            IntLiteralError::TooLarge => "integer literal is too large".to_owned(),
        }
    }
}

/// Parses an `INT_NUMBER` token: optional radix prefix `0x`/`0o`/`0b`, digits and `_`, then an
/// optional suffix. In a hexadecimal literal the suffix starts at the first `[g-zG-Z]`; in
/// other radixes at the first character that is not `[0-9_]`.
pub fn parse_int_literal(text: &str) -> Result<(u128, Option<&str>), IntLiteralError> {
    let (radix, body) = if let Some(rest) = text.strip_prefix("0x") {
        (16u32, rest)
    } else if let Some(rest) = text.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = text.strip_prefix("0b") {
        (2, rest)
    } else {
        (10, text)
    };
    let suffix_start = body
        .char_indices()
        .find(|&(_, c)| {
            if radix == 16 {
                !(c.is_ascii_hexdigit() || c == '_')
            } else {
                !(c.is_ascii_digit() || c == '_')
            }
        })
        .map_or(body.len(), |(i, _)| i);
    let (digits, suffix) = body.split_at(suffix_start);
    let mut value = Some(0u128);
    let mut any_digit = false;
    for c in digits.chars().filter(|c| *c != '_') {
        let Some(digit) = c.to_digit(radix) else {
            return Err(IntLiteralError::InvalidDigit { digit: c, radix });
        };
        any_digit = true;
        value = value
            .and_then(|v| v.checked_mul(u128::from(radix)))
            .and_then(|v| v.checked_add(u128::from(digit)));
    }
    if !any_digit {
        return Err(IntLiteralError::NoDigits);
    }
    if !suffix.is_empty() && !INT_SUFFIXES.contains(&suffix) {
        return Err(IntLiteralError::InvalidSuffix(suffix.to_owned()));
    }
    let value = value.ok_or(IntLiteralError::TooLarge)?;
    Ok((value, (!suffix.is_empty()).then_some(suffix)))
}

/// Unescapes a string literal token (quotes included). Supported escapes are
/// `\n \r \t \\ \" \0 \u{X…}`. On failure returns the first error with its range relative to
/// the start of `text`.
pub fn unescape_string(text: &str) -> Result<String, (TextRange, String)> {
    let whole = TextRange::up_to(text_size(text.len()));
    let Some(body) = text.strip_prefix('"') else {
        return Err((whole, "expected a string literal".to_owned()));
    };
    // Offsets below are relative to `body`; `rel` shifts them past the opening quote.
    let rel = |start: usize, end: usize| {
        TextRange::new(
            text_size(start.saturating_add(1)),
            text_size(end.saturating_add(1)),
        )
    };
    let mut out = String::with_capacity(body.len());
    let mut chars = body.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok(out),
            '\\' => {
                let Some((j, escape)) = chars.next() else {
                    break;
                };
                let after = j.saturating_add(escape.len_utf8());
                let simple = match escape {
                    'n' => Some('\n'),
                    'r' => Some('\r'),
                    't' => Some('\t'),
                    '\\' => Some('\\'),
                    '"' => Some('"'),
                    '0' => Some('\0'),
                    _ => None,
                };
                if let Some(ch) = simple {
                    out.push(ch);
                    continue;
                }
                if escape != 'u' {
                    return Err((
                        rel(i, after),
                        format!("unknown character escape `\\{escape}`"),
                    ));
                }
                let mut end = after;
                if chars.next_if(|&(_, c)| c == '{').is_none() {
                    return Err((rel(i, end), "expected `{` after `\\u`".to_owned()));
                }
                end = end.saturating_add(1);
                let mut value = 0u32;
                let mut digits = 0usize;
                let mut closed = false;
                while let Some((k, c)) = chars.next_if(|&(_, c)| c != '"') {
                    end = k.saturating_add(c.len_utf8());
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    if c == '_' {
                        continue;
                    }
                    let Some(digit) = c.to_digit(16) else {
                        return Err((
                            rel(i, end),
                            "invalid character in unicode escape".to_owned(),
                        ));
                    };
                    digits = digits.saturating_add(1);
                    value = value.saturating_mul(16).saturating_add(digit);
                }
                if !closed {
                    return Err((rel(i, end), "unterminated unicode escape".to_owned()));
                }
                if digits == 0 || digits > 6 {
                    return Err((
                        rel(i, end),
                        "unicode escape must have 1 to 6 hex digits".to_owned(),
                    ));
                }
                let Some(ch) = char::from_u32(value) else {
                    return Err((rel(i, end), "invalid unicode character escape".to_owned()));
                };
                out.push(ch);
            }
            _ => out.push(c),
        }
    }
    Err((whole, "unterminated string literal".to_owned()))
}

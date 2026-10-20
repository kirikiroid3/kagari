//! Literals and string unescaping.

use rowan::TextRange;

use super::tokens;
use crate::represent::cst::{RepresentKind, SyntaxToken};
use crate::represent::lex::text_size;

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
        tokens(&self.0).find(|t| !t.kind().is_trivia())
    }

    pub fn kind(&self) -> Option<LiteralKind> {
        Some(match self.token()?.kind() {
            RepresentKind::INT_NUMBER => LiteralKind::Int,
            RepresentKind::DECIMAL_NUMBER => LiteralKind::Decimal,
            RepresentKind::STRING => LiteralKind::Str,
            RepresentKind::TRUE_KW => LiteralKind::Bool(true),
            RepresentKind::FALSE_KW => LiteralKind::Bool(false),
            _ => return None,
        })
    }

    /// The value of an integer literal; `None` if it is not one or does not fit `u128`.
    pub fn int_value(&self) -> Option<u128> {
        let token = self.token()?;
        if token.kind() != RepresentKind::INT_NUMBER {
            return None;
        }
        parse_digits(token.text())
    }

    /// A decimal literal as the exact fraction `numerator / denominator` (`0.75` → `(75, 100)`).
    pub fn decimal_value(&self) -> Option<(u128, u128)> {
        let token = self.token()?;
        if token.kind() != RepresentKind::DECIMAL_NUMBER {
            return None;
        }
        let (int, frac) = token.text().split_once('.')?;
        let frac_digits: String = frac.chars().filter(|c| *c != '_').collect();
        let numerator = parse_digits(&format!("{int}{frac_digits}"))?;
        let exponent = u32::try_from(frac_digits.len()).ok()?;
        let denominator = 10u128.checked_pow(exponent)?;
        Some((numerator, denominator))
    }

    /// The unescaped value of a string literal.
    pub fn string_value(&self) -> Option<String> {
        let token = self.token()?;
        if token.kind() != RepresentKind::STRING {
            return None;
        }
        unescape_string(token.text()).ok()
    }
}

fn parse_digits(text: &str) -> Option<u128> {
    let digits: String = text.chars().filter(|c| *c != '_').collect();
    digits.parse().ok()
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

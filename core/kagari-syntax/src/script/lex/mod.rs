//! Hand-written two-mode lexer for `.kis`.
//!
//! Content mode yields character data, references and markup openers. Tag mode starts after `<`
//! or `</` and runs until `>`/`/>`; it yields names, whitespace, `=` and attribute values.

use crate::{SyntaxError, TextRange, TextSize};

use super::ScriptKind;

/// Lexes `text` into `(kind, length)` pairs covering the whole input. Lexer errors are dropped;
/// use [`lex_with_errors`] to keep them.
pub fn lex(text: &str) -> Vec<(ScriptKind, TextSize)> {
    lex_with_errors(text).0
}

/// Lexes `text` into `(kind, length)` pairs covering the whole input, plus the lexer errors
/// (unterminated comments, CDATA sections, attribute values and so on).
pub fn lex_with_errors(text: &str) -> (Vec<(ScriptKind, TextSize)>, Vec<SyntaxError>) {
    let mut lexer = Lexer {
        text,
        pos: 0,
        in_tag: false,
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    while lexer.pos < text.len() {
        let rest = text.get(lexer.pos..).unwrap_or("");
        if lexer.in_tag {
            lexer.tag(rest);
        } else {
            lexer.content(rest);
        }
    }
    (lexer.tokens, lexer.errors)
}

/// First character of an element or attribute name.
pub(crate) fn is_name_start(c: char) -> bool {
    c == '_' || c == ':' || unicode_ident::is_xid_start(c)
}

/// Subsequent character of an element or attribute name.
pub(crate) fn is_name_continue(c: char) -> bool {
    matches!(c, '-' | '.' | ':') || unicode_ident::is_xid_continue(c)
}

/// Given the text right after a `&`, returns the byte length of the reference body (between
/// `&` and `;`) if a `;`-terminated reference follows.
pub(crate) fn reference_body_len(after_amp: &str) -> Option<usize> {
    for (index, c) in after_amp.char_indices() {
        if c == ';' {
            return (index > 0).then_some(index);
        }
        if !(is_name_continue(c) || (index == 0 && c == '#')) {
            return None;
        }
    }
    None
}

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn size(len: usize) -> TextSize {
    TextSize::new(u32::try_from(len).unwrap_or(u32::MAX))
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
    in_tag: bool,
    tokens: Vec<(ScriptKind, TextSize)>,
    errors: Vec<SyntaxError>,
}

impl Lexer<'_> {
    /// Emits a token of `len` bytes (at least one char) starting at the current position.
    fn push(&mut self, kind: ScriptKind, len: usize) {
        let rest = self.text.get(self.pos..).unwrap_or("");
        // Never emit an empty token and never split a char.
        let mut len = len.min(rest.len());
        if len == 0 {
            len = rest.chars().next().map_or(rest.len(), char::len_utf8);
        }
        while !rest.is_char_boundary(len) {
            len = len.saturating_add(1);
        }
        self.tokens.push((kind, size(len)));
        self.pos = self.pos.saturating_add(len);
    }

    fn error_here(&mut self, len: usize, message: impl Into<String>) {
        let start = size(self.pos);
        let end = size(self.pos.saturating_add(len));
        self.errors.push(SyntaxError::new(
            message,
            TextRange::new(start, end.max(start)),
        ));
    }

    /// Emits a token that ends after `terminator` (searched from byte `from`), or runs to EOF
    /// with `message` as an error.
    fn delimited(
        &mut self,
        rest: &str,
        kind: ScriptKind,
        from: usize,
        terminator: &str,
        message: &str,
    ) {
        let found = rest
            .get(from..)
            .and_then(|tail| tail.find(terminator))
            .map(|index| from.saturating_add(index).saturating_add(terminator.len()));
        match found {
            Some(len) => self.push(kind, len),
            None => {
                self.error_here(rest.len(), message);
                self.push(kind, rest.len());
            }
        }
    }

    fn content(&mut self, rest: &str) {
        if rest.starts_with("<!--") {
            self.delimited(rest, ScriptKind::COMMENT, 4, "-->", "unterminated comment");
        } else if rest.starts_with("<![CDATA[") {
            self.delimited(
                rest,
                ScriptKind::CDATA,
                9,
                "]]>",
                "unterminated CDATA section",
            );
        } else if rest.starts_with("<!DOCTYPE") {
            self.doctype(rest);
        } else if rest.starts_with("<!") {
            let len = rest
                .find('>')
                .map_or(rest.len(), |index| index.saturating_add(1));
            self.error_here(len, "unsupported markup declaration");
            self.push(ScriptKind::ERROR_TOKEN, len);
        } else if let Some(after) = rest.strip_prefix("<?") {
            let target_len = after
                .find(|c: char| is_xml_space(c) || c == '?')
                .unwrap_or(after.len());
            let kind = if after.get(..target_len) == Some("xml") {
                ScriptKind::XML_DECL
            } else {
                ScriptKind::PI
            };
            self.delimited(rest, kind, 2, "?>", "unterminated processing instruction");
        } else if rest.starts_with("</") {
            self.push(ScriptKind::L_ANGLE_SLASH, 2);
            self.in_tag = true;
        } else if let Some(after) = rest.strip_prefix('<') {
            if after.chars().next().is_some_and(is_name_start) {
                self.push(ScriptKind::L_ANGLE, 1);
                self.in_tag = true;
            } else {
                self.error_here(1, "`<` must be escaped as `&lt;`");
                self.push(ScriptKind::ERROR_TOKEN, 1);
            }
        } else if let Some(after) = rest.strip_prefix('&') {
            match reference_body_len(after) {
                Some(body) => self.push(ScriptKind::ENTITY_REF, body.saturating_add(2)),
                None => {
                    self.error_here(1, "`&` must be escaped as `&amp;`");
                    self.push(ScriptKind::ERROR_TOKEN, 1);
                }
            }
        } else {
            let len = rest.find(['<', '&']).unwrap_or(rest.len());
            self.push(ScriptKind::TEXT, len);
        }
    }

    /// `<!DOCTYPE …>`, skipping over an internal subset in `[…]` and quoted literals.
    fn doctype(&mut self, rest: &str) {
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        let mut end = None;
        for (index, c) in rest.char_indices() {
            match (quote, c) {
                (Some(q), _) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '[') => depth = depth.saturating_add(1),
                (None, ']') => depth = depth.saturating_sub(1),
                (None, '>') if depth == 0 => {
                    end = Some(index.saturating_add(1));
                    break;
                }
                _ => {}
            }
        }
        match end {
            Some(len) => self.push(ScriptKind::DOCTYPE, len),
            None => {
                self.error_here(rest.len(), "unterminated DOCTYPE declaration");
                self.push(ScriptKind::DOCTYPE, rest.len());
            }
        }
    }

    fn tag(&mut self, rest: &str) {
        let Some(first) = rest.chars().next() else {
            return;
        };
        match first {
            c if is_xml_space(c) => {
                let len = rest.find(|c: char| !is_xml_space(c)).unwrap_or(rest.len());
                self.push(ScriptKind::WHITESPACE, len);
            }
            '>' => {
                self.push(ScriptKind::R_ANGLE, 1);
                self.in_tag = false;
            }
            '/' if rest.starts_with("/>") => {
                self.push(ScriptKind::SLASH_R_ANGLE, 2);
                self.in_tag = false;
            }
            '=' => self.push(ScriptKind::EQ, 1),
            '"' | '\'' => {
                let found = rest
                    .get(1..)
                    .and_then(|tail| tail.find(first))
                    .map(|index| index.saturating_add(2));
                match found {
                    Some(len) => self.push(ScriptKind::ATTR_VALUE, len),
                    None => {
                        self.error_here(rest.len(), "unterminated attribute value");
                        self.push(ScriptKind::ATTR_VALUE, rest.len());
                    }
                }
            }
            // A new tag starts before this one was closed: leave tag mode and let content mode
            // lex the `<`. The parser reports the missing `>`.
            '<' => self.in_tag = false,
            c if is_name_start(c) => {
                let len = rest
                    .char_indices()
                    .skip(1)
                    .find(|&(_, c)| !is_name_continue(c))
                    .map_or(rest.len(), |(index, _)| index);
                self.push(ScriptKind::NAME, len);
            }
            _ => {
                let len = rest
                    .char_indices()
                    .skip(1)
                    .find(|&(_, c)| {
                        is_xml_space(c)
                            || is_name_start(c)
                            || matches!(c, '>' | '/' | '=' | '"' | '\'' | '<')
                    })
                    .map_or(rest.len(), |(index, _)| index);
                self.error_here(len, "unexpected character in tag");
                self.push(ScriptKind::ERROR_TOKEN, len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScriptKind, lex_with_errors};

    #[test]
    fn covers_input() {
        let inputs = [
            "<a b='1' c=\"2\">x &amp; y<![CDATA[<z>]]><!-- c --></a>",
            "<a b=\"unterminated",
            "< a & <!-- x",
            "<a <b>",
            "<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY y \">\">]><r/>",
            "<a\u{3042}=\"\u{3042}\">\u{3042}</a\u{3042}>",
        ];
        for input in inputs {
            let (tokens, _) = lex_with_errors(input);
            let total: u32 = tokens
                .iter()
                .fold(0u32, |acc, (_, len)| acc.saturating_add(u32::from(*len)));
            assert_eq!(usize::try_from(total).ok(), Some(input.len()), "{input}");
            assert!(tokens.iter().all(|(_, len)| u32::from(*len) > 0));
        }
    }

    #[test]
    fn modes() {
        let (tokens, errors) = lex_with_errors("<a b='1'/>t");
        let kinds: Vec<_> = tokens.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(
            kinds,
            [
                ScriptKind::L_ANGLE,
                ScriptKind::NAME,
                ScriptKind::WHITESPACE,
                ScriptKind::NAME,
                ScriptKind::EQ,
                ScriptKind::ATTR_VALUE,
                ScriptKind::SLASH_R_ANGLE,
                ScriptKind::TEXT,
            ]
        );
        assert!(errors.is_empty());
    }
}

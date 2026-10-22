//! Syntactic validation: form names, delimiters and entries, and literal well-formedness.

use rowan::ast::AstNode;
use rowan::{TextRange, WalkEvent};

use crate::ast::{self, FormDelimiter, FormEntry, HasName, unescape_string};
use crate::cst::{SyntaxKind, SyntaxNode};
use crate::{SourceKind, SyntaxError};

/// The known forms and their delimiters.
const FORMS: &[(&str, FormDelimiter)] = &[
    ("s", FormDelimiter::Paren),
    ("choice", FormDelimiter::Brace),
    ("t", FormDelimiter::Bracket),
    ("column", FormDelimiter::Bracket),
    ("row", FormDelimiter::Bracket),
    ("layer", FormDelimiter::Bracket),
    ("text", FormDelimiter::Paren),
    ("image", FormDelimiter::Paren),
    ("game_screen", FormDelimiter::Paren),
    ("button", FormDelimiter::Brace),
    ("slider", FormDelimiter::Brace),
    ("toggle", FormDelimiter::Brace),
    ("select", FormDelimiter::Brace),
];

fn is_form(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::FORM_EXPR | SyntaxKind::TEXT_FORM)
}

pub(crate) fn validate(root: &SyntaxNode, kind: SourceKind, errors: &mut Vec<SyntaxError>) {
    for event in root.preorder_with_tokens() {
        let WalkEvent::Enter(element) = event else {
            continue;
        };
        match element {
            rowan::NodeOrToken::Node(node) => {
                if is_form(node.kind()) {
                    validate_form(&node, kind, errors);
                }
            }
            rowan::NodeOrToken::Token(token) => match token.kind() {
                SyntaxKind::STRING => {
                    if let Err((rel, message)) = unescape_string(token.text())
                        && let Some(range) = rel.checked_add(token.text_range().start())
                    {
                        errors.push(SyntaxError::new(message, range));
                    }
                }
                SyntaxKind::INT_NUMBER => {
                    if let Err(error) = ast::parse_int_literal(token.text()) {
                        errors.push(SyntaxError::new(error.message(), token.text_range()));
                    }
                }
                _ => {}
            },
        }
    }
}

fn validate_form(node: &SyntaxNode, kind: SourceKind, errors: &mut Vec<SyntaxError>) {
    if kind == SourceKind::Kis && !node.ancestors().skip(1).any(|a| is_form(a.kind())) {
        errors.push(SyntaxError::new(
            "forms are only allowed in `.kix` files",
            node.text_range(),
        ));
    }
    // `t![…]` is always the known text form with its own delimiter.
    let Some(form) = ast::FormExpr::cast(node.clone()) else {
        return;
    };
    let Some(name_ref) = form.name_ref() else {
        return;
    };
    let name = name_ref.text();
    let name_range = name_ref.syntax().text_range();
    let Some(&(_, expected)) = FORMS.iter().find(|(known, _)| *known == name) else {
        errors.push(SyntaxError::new(
            format!("unknown form `{name}!`"),
            name_range,
        ));
        return;
    };
    let Some(delimiter) = form.delimiter() else {
        return;
    };
    if delimiter != expected {
        errors.push(SyntaxError::new(
            format!(
                "`{name}!` is written `{name}!{}…{}`",
                expected.open(),
                expected.close()
            ),
            node.text_range(),
        ));
        return;
    }

    let mut keys: Vec<(String, TextRange, Option<ast::Expr>)> = Vec::new();
    let mut elements: Vec<FormEntry> = Vec::new();
    for entry in form.entries() {
        match &entry {
            FormEntry::FormAttr(attr) => {
                let range = attr.syntax().text_range();
                if delimiter == FormDelimiter::Bracket {
                    errors.push(SyntaxError::new(
                        format!("`{name}![…]` takes no attributes"),
                        range,
                    ));
                    continue;
                }
                if !elements.is_empty() {
                    errors.push(SyntaxError::new(
                        "attributes must come before elements",
                        range,
                    ));
                }
                let Some(key) = attr.name() else {
                    continue;
                };
                let key_text = key.text();
                let key_range = key.syntax().text_range();
                if keys.iter().any(|(k, _, _)| *k == key_text) {
                    errors.push(SyntaxError::new(
                        format!("duplicate attribute `{key_text}`"),
                        key_range,
                    ));
                    continue;
                }
                keys.push((key_text, key_range, attr.value()));
            }
            FormEntry::FormArm(arm) => {
                if name != "choice" {
                    errors.push(SyntaxError::new(
                        "arms are only allowed in `choice!`",
                        arm.syntax().text_range(),
                    ));
                }
                elements.push(entry);
            }
            FormEntry::Expr(_) => elements.push(entry),
        }
    }

    match name.as_str() {
        "s" => {
            check_keys(&name, &keys, &["who", "voice", "render"], errors);
            for (index, element) in elements.iter().enumerate() {
                if index >= 1 {
                    errors.push(SyntaxError::new(
                        "`s!` takes at most one element",
                        element.syntax().text_range(),
                    ));
                } else if let FormEntry::Expr(expr) = element
                    && !matches!(expr, ast::Expr::TextForm(_))
                {
                    errors.push(SyntaxError::new(
                        "the element of `s!` must be `t![…]`",
                        element.syntax().text_range(),
                    ));
                }
            }
        }
        "choice" => {
            check_keys(&name, &keys, &["id", "render"], errors);
            match keys.iter().find(|(key, _, _)| key == "id") {
                None => errors.push(SyntaxError::new(
                    "`choice!` requires an `id` attribute",
                    name_range,
                )),
                Some((_, key_range, value)) => {
                    let is_string = matches!(
                        value,
                        Some(ast::Expr::Literal(lit)) if lit.kind() == Some(ast::LiteralKind::Str)
                    );
                    if !is_string {
                        let range = value
                            .as_ref()
                            .map_or(*key_range, |value| value.syntax().text_range());
                        errors.push(SyntaxError::new("`id` must be a string literal", range));
                    }
                }
            }
            for element in &elements {
                let is_text_arm = match element {
                    FormEntry::FormArm(arm) => matches!(arm.text(), Some(ast::Expr::TextForm(_))),
                    _ => false,
                };
                if !is_text_arm {
                    errors.push(SyntaxError::new(
                        "a `choice!` element must be an arm `t![…] => Variant`",
                        element.syntax().text_range(),
                    ));
                }
            }
        }
        _ => {}
    }
}

fn check_keys(
    name: &str,
    keys: &[(String, TextRange, Option<ast::Expr>)],
    allowed: &[&str],
    errors: &mut Vec<SyntaxError>,
) {
    for (key, range, _) in keys {
        if !allowed.contains(&key.as_str()) {
            errors.push(SyntaxError::new(
                format!("unknown attribute `{key}` for `{name}!`"),
                *range,
            ));
        }
    }
}

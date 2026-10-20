//! Recursive-descent parser for `.kirp`/`.kix` producing a lossless rowan tree.
//!
//! Trivia is attached lazily: pending whitespace and comments are emitted into the current node
//! right before the next significant token or node start, so nodes never begin or end with
//! trivia, and trailing trivia ends up in the root.

use rowan::{Checkpoint, GreenNodeBuilder, Language, TextRange, TextSize};

use super::{RepresentKind, RepresentLanguage};
use crate::represent::ast::unescape_string;
use crate::represent::lex::{lex, text_size};
use crate::{Parse, SyntaxError};

use RepresentKind::*;

/// Parses a whole `.kirp`/`.kix` file into a `SOURCE_FILE`.
pub fn parse_file(text: &str) -> Parse<RepresentLanguage> {
    let mut p = Parser::new(text);
    p.builder.start_node(raw(SOURCE_FILE));
    p.items(false);
    p.finish_root()
}

/// Parses the content of an inline `<r>` element into an `INLINE_RENDER` of statements.
pub fn parse_inline(text: &str) -> Parse<RepresentLanguage> {
    let mut p = Parser::new(text);
    p.builder.start_node(raw(INLINE_RENDER));
    p.stmts(false);
    p.finish_root()
}

fn raw(kind: RepresentKind) -> rowan::SyntaxKind {
    RepresentLanguage::kind_to_raw(kind)
}

const PREFIX_BP: u8 = 13;
const CAST_BP: u8 = 11;

/// `(left, right, is_comparison)` binding powers of a binary operator.
fn infix_bp(kind: RepresentKind) -> Option<(u8, u8, bool)> {
    Some(match kind {
        PIPE2 => (1, 2, false),
        AMP2 => (3, 4, false),
        EQ2 | NEQ | L_ANGLE | R_ANGLE | LTEQ | GTEQ => (5, 6, true),
        PLUS | MINUS => (7, 8, false),
        STAR | SLASH | PERCENT => (9, 10, false),
        _ => return None,
    })
}

fn is_item_start(kind: RepresentKind) -> bool {
    matches!(
        kind,
        FN_KW | STRUCT_KW | ENUM_KW | TYPE_KW | CONST_KW | MOD_KW | IMPL_KW | IMPORT_KW | POUND
    )
}

fn is_block_like(kind: Option<RepresentKind>) -> bool {
    matches!(kind, Some(BLOCK_EXPR | MATCH_EXPR))
}

struct Parser<'t> {
    text: &'t str,
    tokens: Vec<(RepresentKind, TextRange)>,
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

    fn finish_root(mut self) -> Parse<RepresentLanguage> {
        // Anything left (only possible after a bug) is kept for losslessness.
        while self.pos < self.tokens.len() {
            self.bump();
        }
        self.flush();
        self.builder.finish_node();
        let mut errors = self.errors;
        errors.sort_by_key(|error| error.range.start());
        Parse::new(self.builder.finish(), errors)
    }

    // ---- token access -------------------------------------------------------------------

    fn significant(&self) -> impl Iterator<Item = &(RepresentKind, TextRange)> {
        self.tokens
            .get(self.pos..)
            .unwrap_or_default()
            .iter()
            .filter(|(kind, _)| !kind.is_trivia())
    }

    fn nth(&self, n: usize) -> Option<RepresentKind> {
        self.significant().nth(n).map(|(kind, _)| *kind)
    }

    fn current(&self) -> Option<RepresentKind> {
        self.nth(0)
    }

    fn at(&self, kind: RepresentKind) -> bool {
        self.current() == Some(kind)
    }

    fn nth_at(&self, n: usize, kind: RepresentKind) -> bool {
        self.nth(n) == Some(kind)
    }

    fn current_range(&self) -> TextRange {
        self.significant()
            .next()
            .map(|(_, range)| *range)
            .unwrap_or_else(|| TextRange::empty(text_size(self.text.len())))
    }

    fn current_text(&self) -> &'t str {
        let range = self.current_range();
        self.text
            .get(usize::from(range.start())..usize::from(range.end()))
            .unwrap_or_default()
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

    fn push_token(&mut self, kind: RepresentKind, range: TextRange) {
        let text = self
            .text
            .get(usize::from(range.start())..usize::from(range.end()))
            .unwrap_or_default();
        self.builder.token(raw(kind), text);
        self.pos = self.pos.saturating_add(1);
    }

    /// Consumes the next significant token (after its leading trivia).
    fn bump(&mut self) {
        self.flush();
        let Some(&(kind, range)) = self.tokens.get(self.pos) else {
            return;
        };
        self.push_token(kind, range);
        self.last_end = range.end();
        if kind == STRING {
            let text = self
                .text
                .get(usize::from(range.start())..usize::from(range.end()))
                .unwrap_or_default();
            if let Err((rel, message)) = unescape_string(text)
                && let Some(abs) = rel.checked_add(range.start())
            {
                self.errors.push(SyntaxError::new(message, abs));
            }
        }
    }

    fn eat(&mut self, kind: RepresentKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: RepresentKind) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error_missing(format!("expected {}", kind.describe()));
            false
        }
    }

    fn start(&mut self, kind: RepresentKind) {
        self.flush();
        self.builder.start_node(raw(kind));
    }

    fn start_at(&mut self, checkpoint: Checkpoint, kind: RepresentKind) {
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
    fn err_missing_node(&mut self, message: &str, stop: &[RepresentKind]) {
        self.error(message);
        self.start(ERROR);
        match self.current() {
            Some(kind) if !is_item_start(kind) && !stop.contains(&kind) => self.bump(),
            _ => {}
        }
        self.finish();
    }

    // ---- items --------------------------------------------------------------------------

    fn items(&mut self, nested: bool) {
        loop {
            match self.current() {
                None => break,
                Some(R_CURLY) if nested => break,
                Some(kind) if is_item_start(kind) => self.item(),
                Some(_) => {
                    self.error("expected an item");
                    self.start(ERROR);
                    loop {
                        self.bump();
                        match self.current() {
                            None | Some(R_CURLY) => break,
                            Some(kind) if is_item_start(kind) => break,
                            Some(_) => {}
                        }
                    }
                    self.finish();
                }
            }
        }
    }

    fn item(&mut self) {
        let cp = self.checkpoint();
        while self.at(POUND) {
            self.attr();
        }
        let kind = match self.current() {
            Some(FN_KW) => FN,
            Some(STRUCT_KW) => STRUCT,
            Some(ENUM_KW) => ENUM,
            Some(TYPE_KW) => TYPE_ALIAS,
            Some(CONST_KW) => CONST,
            Some(MOD_KW) => MOD,
            Some(IMPL_KW) => IMPL,
            Some(IMPORT_KW) => ASSET_IMPORT,
            _ => {
                self.start_at(cp, ERROR);
                self.error("expected an item after attributes");
                self.finish();
                return;
            }
        };
        self.start_at(cp, kind);
        self.bump();
        match kind {
            FN => self.fn_rest(),
            STRUCT => self.struct_rest(),
            ENUM => self.enum_rest(),
            TYPE_ALIAS => self.type_alias_rest(),
            CONST => self.const_rest(),
            MOD => self.mod_rest(),
            IMPL => self.impl_rest(),
            _ => self.asset_import_rest(),
        }
        self.finish();
    }

    fn attr(&mut self) {
        self.start(ATTR);
        self.bump();
        if self.at(BANG) {
            self.err_bump("inner attributes are not supported");
        }
        if self.expect(L_BRACK) {
            if self.at(IDENT) {
                self.name_ref();
            } else {
                self.error_missing("expected an attribute name");
            }
            if self.at(L_PAREN) {
                self.token_tree();
            }
            self.expect(R_BRACK);
        }
        self.finish();
    }

    fn token_tree(&mut self) {
        self.start(TOKEN_TREE);
        self.bump();
        let mut depth = 0usize;
        loop {
            match self.current() {
                None => {
                    self.error_missing("expected `)`");
                    break;
                }
                Some(L_PAREN | L_BRACK | L_CURLY) => {
                    depth = depth.saturating_add(1);
                    self.bump();
                }
                Some(R_PAREN | R_BRACK | R_CURLY) if depth == 0 => {
                    self.expect(R_PAREN);
                    break;
                }
                Some(R_PAREN | R_BRACK | R_CURLY) => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                }
                Some(_) => self.bump(),
            }
        }
        self.finish();
    }

    fn name(&mut self) {
        if self.at(IDENT) {
            self.start(NAME);
            self.bump();
            self.finish();
        } else {
            self.error_missing("expected a name");
        }
    }

    fn name_ref(&mut self) {
        self.start(NAME_REF);
        self.bump();
        self.finish();
    }

    fn fn_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        if self.at(L_PAREN) {
            self.param_list();
        } else {
            self.error_missing("expected `(`");
        }
        if self.at(THIN_ARROW) {
            self.start(RET_TYPE);
            self.bump();
            self.ty();
            self.finish();
        }
        if self.at(L_CURLY) {
            self.block_expr();
        } else if !self.eat(SEMICOLON) {
            self.error_missing("expected `{` or `;`");
        }
    }

    fn generic_param_list(&mut self) {
        self.start(GENERIC_PARAM_LIST);
        self.bump();
        while self.at(IDENT) {
            self.start(GENERIC_PARAM);
            self.name();
            if self.at(COLON) {
                self.error("trait bounds are not supported");
                self.start(ERROR);
                self.bump();
                self.ty();
                self.finish();
            }
            self.finish();
            if !self.eat(COMMA) {
                break;
            }
        }
        self.expect(R_ANGLE);
        self.finish();
    }

    fn param_list(&mut self) {
        self.start(PARAM_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_PAREN | L_CURLY) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(AMP | AMP2) => {
                    self.err_bump("references are not supported");
                    continue;
                }
                Some(SELF_KW) => {
                    self.start(SELF_PARAM);
                    self.bump();
                    if self.at(COLON) {
                        self.error("explicit `self` types are not supported");
                        self.start(ERROR);
                        self.bump();
                        self.ty();
                        self.finish();
                    }
                    self.finish();
                }
                Some(_) => {
                    self.start(PARAM);
                    self.pat_top();
                    if self.expect(COLON) {
                        self.ty();
                    }
                    self.finish();
                }
            }
            if !self.eat(COMMA) {
                break;
            }
        }
        self.expect(R_PAREN);
        self.finish();
    }

    fn struct_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        match self.current() {
            Some(L_CURLY) => self.record_field_list(),
            Some(SEMICOLON) => self.bump(),
            Some(L_PAREN) => {
                self.error("tuple structs are not supported");
                self.start(ERROR);
                self.tuple_field_list();
                self.finish();
                self.eat(SEMICOLON);
            }
            _ => self.error_missing("expected `{` or `;`"),
        }
    }

    fn record_field_list(&mut self) {
        self.start(RECORD_FIELD_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(IDENT) => {
                    self.start(RECORD_FIELD);
                    self.name();
                    if self.expect(COLON) {
                        self.ty();
                    }
                    self.finish();
                }
                Some(_) => {
                    self.err_bump("expected a field");
                    continue;
                }
            }
            if !self.list_comma(IDENT) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    /// After a list element: eats `,` and tells whether the list goes on. A missing comma is
    /// reported only when another element follows.
    fn list_comma(&mut self, next_element: RepresentKind) -> bool {
        if self.eat(COMMA) {
            return true;
        }
        if self.at(next_element) {
            self.error_missing("expected `,`");
            return true;
        }
        false
    }

    fn enum_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        if !self.at(L_CURLY) {
            self.error_missing("expected `{`");
            return;
        }
        self.start(VARIANT_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(IDENT) => self.variant(),
                Some(_) => {
                    self.err_bump("expected a variant");
                    continue;
                }
            }
            if !self.list_comma(IDENT) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    fn variant(&mut self) {
        self.start(VARIANT);
        self.name();
        match self.current() {
            Some(L_PAREN) => self.tuple_field_list(),
            Some(L_CURLY) => {
                self.error("record variants are not supported");
                self.start(ERROR);
                self.record_field_list();
                self.finish();
            }
            _ => {}
        }
        if self.at(EQ) {
            self.error("explicit discriminants are not supported");
            self.start(ERROR);
            self.bump();
            self.expr();
            self.finish();
        }
        self.finish();
    }

    fn tuple_field_list(&mut self) {
        self.start(TUPLE_FIELD_LIST);
        self.bump();
        while self.at_type_start() {
            self.ty();
            if !self.eat(COMMA) {
                break;
            }
        }
        self.expect(R_PAREN);
        self.finish();
    }

    fn type_alias_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        if self.expect(EQ) || self.at_type_start() {
            self.ty();
        }
        self.expect(SEMICOLON);
    }

    fn const_rest(&mut self) {
        self.name();
        if self.expect(COLON) {
            self.ty();
        }
        if self.expect(EQ) {
            self.expr();
        }
        self.expect(SEMICOLON);
    }

    fn mod_rest(&mut self) {
        self.name();
        if self.at(L_CURLY) {
            self.item_list();
        } else {
            self.error_missing("expected `{`");
            self.eat(SEMICOLON);
        }
    }

    fn impl_rest(&mut self) {
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        self.ty();
        if self.at(L_CURLY) {
            self.item_list();
        } else {
            self.error_missing("expected `{`");
        }
    }

    fn item_list(&mut self) {
        self.start(ITEM_LIST);
        self.bump();
        self.items(true);
        self.expect(R_CURLY);
        self.finish();
    }

    fn asset_import_rest(&mut self) {
        self.name();
        if self.at(IDENT) && self.current_text() == "from" {
            self.bump();
        } else {
            self.error_missing("expected `from`");
        }
        if !self.eat(STRING) {
            self.error_missing("expected a string literal");
        }
        if self.expect(AS_KW) {
            self.ty();
        }
        self.expect(SEMICOLON);
    }

    // ---- types and paths ----------------------------------------------------------------

    fn at_type_start(&self) -> bool {
        matches!(
            self.current(),
            Some(IDENT | SELF_TYPE_KW | SELF_KW | L_PAREN | AMP | AMP2)
        )
    }

    fn ty(&mut self) {
        match self.current() {
            Some(AMP | AMP2) => {
                self.err_bump("references are not supported");
                self.ty();
            }
            Some(L_PAREN) if self.nth_at(1, R_PAREN) => {
                self.start(UNIT_TYPE);
                self.bump();
                self.bump();
                self.finish();
            }
            Some(L_PAREN) => {
                self.error("tuple types are not supported");
                self.start(ERROR);
                self.bump();
                while self.at_type_start() {
                    self.ty();
                    if !self.eat(COMMA) {
                        break;
                    }
                }
                self.expect(R_PAREN);
                self.finish();
            }
            Some(IDENT | SELF_TYPE_KW | SELF_KW) => {
                if self.at(SELF_KW) && !self.nth_at(1, COLON2) {
                    self.error("`self` is not a type; references are not supported");
                }
                self.start(PATH_TYPE);
                self.path(true);
                self.finish();
            }
            _ => self.err_missing_node(
                "expected a type",
                &[
                    R_PAREN, R_CURLY, R_BRACK, R_ANGLE, L_CURLY, COMMA, SEMICOLON, EQ,
                ],
            ),
        }
    }

    /// A `::`-separated path. In type position (`type_args`), generic arguments follow a
    /// segment directly; elsewhere only the turbofish `::<…>` form is accepted.
    fn path(&mut self, type_args: bool) {
        self.start(PATH);
        loop {
            self.start(PATH_SEGMENT);
            match self.current() {
                Some(IDENT) => self.name_ref(),
                Some(SELF_KW | SELF_TYPE_KW) => self.bump(),
                _ => self.error_missing("expected a path segment"),
            }
            if self.at(COLON2) && self.nth_at(1, L_ANGLE) {
                self.generic_arg_list(true);
            } else if type_args && self.at(L_ANGLE) {
                self.generic_arg_list(false);
            }
            self.finish();
            if self.at(COLON2) && matches!(self.nth(1), Some(IDENT | SELF_KW | SELF_TYPE_KW)) {
                self.bump();
            } else {
                break;
            }
        }
        self.finish();
    }

    fn generic_arg_list(&mut self, turbofish: bool) {
        self.start(GENERIC_ARG_LIST);
        if turbofish {
            self.bump();
        }
        self.bump();
        while self.at_type_start() {
            self.ty();
            if !self.eat(COMMA) {
                break;
            }
        }
        self.expect(R_ANGLE);
        self.finish();
    }

    // ---- statements ---------------------------------------------------------------------

    fn block_expr(&mut self) {
        self.start(BLOCK_EXPR);
        self.bump();
        self.stmts(true);
        self.expect(R_CURLY);
        self.finish();
    }

    /// Statements of a block (`in_block`) or of an inline render. In a block a trailing
    /// expression without `;` is the tail expression; in an inline render it is an error.
    fn stmts(&mut self, in_block: bool) {
        loop {
            match self.current() {
                None => break,
                Some(R_CURLY) if in_block => break,
                Some(R_CURLY) => self.err_bump("unexpected `}`"),
                Some(SEMICOLON) => self.bump(),
                Some(LET_KW) => self.let_stmt(),
                Some(kind) if is_item_start(kind) => {
                    if in_block {
                        // Most likely an unclosed block: let the item level take over.
                        break;
                    }
                    self.error("items are not allowed in inline representing code");
                    self.item();
                }
                Some(MATCH_KW | L_CURLY) => {
                    let cp = self.checkpoint();
                    if self.at(MATCH_KW) {
                        self.match_expr();
                    } else {
                        self.block_expr();
                    }
                    if !(in_block && self.at(R_CURLY)) {
                        self.start_at(cp, EXPR_STMT);
                        self.eat(SEMICOLON);
                        self.finish();
                    }
                }
                Some(_) => {
                    let cp = self.checkpoint();
                    self.expr();
                    if self.at(SEMICOLON) {
                        self.start_at(cp, EXPR_STMT);
                        self.bump();
                        self.finish();
                    } else if !(in_block && self.at(R_CURLY)) {
                        self.start_at(cp, EXPR_STMT);
                        self.error_missing("expected `;`");
                        self.finish();
                        self.recover_stmt();
                    }
                }
            }
        }
    }

    /// After a missing `;`: if the next token cannot start a statement, skip to the next `;`
    /// (consumed) or `}`.
    fn recover_stmt(&mut self) {
        match self.current() {
            None | Some(R_CURLY | LET_KW) => return,
            Some(kind) if is_item_start(kind) || Self::is_expr_start(kind) => return,
            Some(_) => {}
        }
        self.start(ERROR);
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(SEMICOLON) => {
                    self.bump();
                    break;
                }
                Some(_) => self.bump(),
            }
        }
        self.finish();
    }

    fn let_stmt(&mut self) {
        self.start(LET_STMT);
        self.bump();
        self.pat_top();
        if self.eat(COLON) {
            self.ty();
        }
        if self.expect(EQ) {
            self.expr();
        }
        let terminated = self.expect(SEMICOLON);
        self.finish();
        if !terminated {
            self.recover_stmt();
        }
    }

    // ---- expressions --------------------------------------------------------------------

    fn is_expr_start(kind: RepresentKind) -> bool {
        matches!(
            kind,
            INT_NUMBER
                | DECIMAL_NUMBER
                | STRING
                | TRUE_KW
                | FALSE_KW
                | IDENT
                | SELF_KW
                | SELF_TYPE_KW
                | L_PAREN
                | L_CURLY
                | MATCH_KW
                | MINUS
                | BANG
                | AMP
                | AMP2
        )
    }

    fn expr(&mut self) -> Option<RepresentKind> {
        self.expr_bp(0, false)
    }

    /// Pratt loop. Returns the kind of the outermost node built, or `None` if no expression
    /// could be parsed (an `ERROR` node was left instead).
    fn expr_bp(&mut self, min_bp: u8, no_struct: bool) -> Option<RepresentKind> {
        let cp = self.checkpoint();
        let mut kind = self.lhs(no_struct)?;
        let mut after_comparison = false;
        while let Some(op) = self.current() {
            if op == AS_KW {
                if CAST_BP < min_bp {
                    break;
                }
                self.start_at(cp, CAST_EXPR);
                self.bump();
                self.ty();
                self.finish();
                kind = CAST_EXPR;
                after_comparison = false;
                continue;
            }
            let Some((left, right, comparison)) = infix_bp(op) else {
                break;
            };
            if left < min_bp {
                break;
            }
            if comparison && after_comparison {
                self.error("comparison operators cannot be chained");
            }
            self.start_at(cp, BIN_EXPR);
            self.bump();
            self.expr_bp(right, no_struct);
            self.finish();
            kind = BIN_EXPR;
            after_comparison = comparison;
        }
        Some(kind)
    }

    fn lhs(&mut self, no_struct: bool) -> Option<RepresentKind> {
        match self.current() {
            Some(MINUS | BANG) => {
                self.start(PREFIX_EXPR);
                self.bump();
                self.expr_bp(PREFIX_BP, no_struct);
                self.finish();
                Some(PREFIX_EXPR)
            }
            Some(AMP | AMP2) => {
                self.err_bump("references are not supported");
                self.lhs(no_struct)
            }
            _ => self.postfix(no_struct),
        }
    }

    fn postfix(&mut self, no_struct: bool) -> Option<RepresentKind> {
        let cp = self.checkpoint();
        let mut kind = self.primary(no_struct)?;
        loop {
            match self.current() {
                Some(L_PAREN) => {
                    self.start_at(cp, CALL_EXPR);
                    self.arg_list();
                    self.finish();
                    kind = CALL_EXPR;
                }
                Some(DOT) => {
                    let method = self.nth_at(1, IDENT)
                        && (self.nth_at(2, L_PAREN)
                            || (self.nth_at(2, COLON2) && self.nth_at(3, L_ANGLE)));
                    kind = if method { METHOD_CALL_EXPR } else { FIELD_EXPR };
                    self.start_at(cp, kind);
                    self.bump();
                    match self.current() {
                        Some(IDENT) => self.name_ref(),
                        Some(INT_NUMBER | DECIMAL_NUMBER) => {
                            self.err_bump("tuple fields are not supported");
                        }
                        _ => self.error_missing("expected a field or method name"),
                    }
                    if method {
                        if self.at(COLON2) {
                            self.generic_arg_list(true);
                        }
                        if self.at(L_PAREN) {
                            self.arg_list();
                        } else {
                            self.error_missing("expected `(`");
                        }
                    }
                    self.finish();
                }
                _ => break,
            }
        }
        Some(kind)
    }

    fn primary(&mut self, no_struct: bool) -> Option<RepresentKind> {
        match self.current() {
            Some(INT_NUMBER | DECIMAL_NUMBER | STRING | TRUE_KW | FALSE_KW) => {
                self.start(LITERAL);
                self.bump();
                self.finish();
                Some(LITERAL)
            }
            Some(IDENT | SELF_KW | SELF_TYPE_KW) => {
                let cp = self.checkpoint();
                self.path(false);
                if !no_struct && self.at(L_CURLY) {
                    self.start_at(cp, RECORD_EXPR);
                    self.record_expr_field_list();
                    self.finish();
                    Some(RECORD_EXPR)
                } else {
                    self.start_at(cp, PATH_EXPR);
                    self.finish();
                    Some(PATH_EXPR)
                }
            }
            Some(L_PAREN) if self.nth_at(1, R_PAREN) => {
                self.start(UNIT_EXPR);
                self.bump();
                self.bump();
                self.finish();
                Some(UNIT_EXPR)
            }
            Some(L_PAREN) => {
                self.start(PAREN_EXPR);
                self.bump();
                self.expr();
                if self.at(COMMA) {
                    self.error("tuples are not supported");
                    self.start(ERROR);
                    while self.eat(COMMA) {
                        if self.at(R_PAREN) {
                            break;
                        }
                        self.expr();
                    }
                    self.finish();
                }
                self.expect(R_PAREN);
                self.finish();
                Some(PAREN_EXPR)
            }
            Some(L_CURLY) => {
                self.block_expr();
                Some(BLOCK_EXPR)
            }
            Some(MATCH_KW) => {
                self.match_expr();
                Some(MATCH_EXPR)
            }
            _ => {
                self.err_missing_node(
                    "expected an expression",
                    &[R_PAREN, R_CURLY, R_BRACK, COMMA, SEMICOLON, FAT_ARROW],
                );
                None
            }
        }
    }

    fn record_expr_field_list(&mut self) {
        self.start(RECORD_EXPR_FIELD_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(DOT2) => {
                    self.bump();
                    self.expr();
                    self.eat(COMMA);
                    break;
                }
                Some(IDENT) => {
                    self.start(RECORD_EXPR_FIELD);
                    self.name_ref();
                    if self.eat(COLON) {
                        self.expr();
                    }
                    self.finish();
                }
                Some(_) => {
                    self.err_bump("expected a field");
                    continue;
                }
            }
            if !self.eat(COMMA) {
                if self.at(IDENT) || self.at(DOT2) {
                    self.error_missing("expected `,`");
                    continue;
                }
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    fn arg_list(&mut self) {
        self.start(ARG_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_PAREN) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(_) => {}
            }
            self.expr();
            if !self.eat(COMMA) {
                break;
            }
        }
        self.expect(R_PAREN);
        self.finish();
    }

    fn match_expr(&mut self) {
        self.start(MATCH_EXPR);
        self.bump();
        self.expr_bp(0, true);
        if self.at(L_CURLY) {
            self.match_arm_list();
        } else {
            self.error_missing("expected `{`");
        }
        self.finish();
    }

    fn match_arm_list(&mut self) {
        self.start(MATCH_ARM_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(_) => {}
            }
            if !self.at_pat_start() {
                self.err_bump("expected a match arm");
                continue;
            }
            self.start(MATCH_ARM);
            self.pat_top();
            let body = if self.expect(FAT_ARROW) {
                self.expr()
            } else {
                None
            };
            if !self.eat(COMMA) && !self.at(R_CURLY) && !is_block_like(body) {
                self.error_missing("expected `,`");
            }
            self.finish();
        }
        self.expect(R_CURLY);
        self.finish();
    }

    // ---- patterns -----------------------------------------------------------------------

    fn at_pat_start(&self) -> bool {
        matches!(
            self.current(),
            Some(
                UNDERSCORE
                    | MINUS
                    | INT_NUMBER
                    | DECIMAL_NUMBER
                    | STRING
                    | TRUE_KW
                    | FALSE_KW
                    | IDENT
                    | SELF_KW
                    | SELF_TYPE_KW
                    | AMP
                    | AMP2
            )
        )
    }

    fn pat_top(&mut self) {
        let cp = self.checkpoint();
        self.pat_atom();
        if self.at(PIPE) {
            self.start_at(cp, OR_PAT);
            while self.eat(PIPE) {
                self.pat_atom();
            }
            self.finish();
        }
    }

    fn pat_atom(&mut self) {
        match self.current() {
            Some(UNDERSCORE) => {
                self.start(WILDCARD_PAT);
                self.bump();
                self.finish();
            }
            Some(MINUS | INT_NUMBER | TRUE_KW | FALSE_KW) => {
                let cp = self.checkpoint();
                self.literal_pat();
                if self.at(DOT2EQ) || self.at(DOT2) {
                    if self.at(DOT2) {
                        self.error("exclusive range patterns are not supported; use `..=`");
                    }
                    self.start_at(cp, RANGE_PAT);
                    self.bump();
                    if matches!(self.current(), Some(MINUS | INT_NUMBER)) {
                        self.literal_pat();
                    } else {
                        self.error_missing("expected a range end");
                    }
                    self.finish();
                }
            }
            Some(IDENT) if !matches!(self.nth(1), Some(COLON2 | L_PAREN | L_CURLY)) => {
                self.start(IDENT_PAT);
                self.name();
                self.finish();
            }
            Some(IDENT | SELF_KW | SELF_TYPE_KW) => {
                let cp = self.checkpoint();
                self.path(false);
                match self.current() {
                    Some(L_PAREN) => {
                        self.start_at(cp, TUPLE_STRUCT_PAT);
                        self.bump();
                        while self.at_pat_start() {
                            self.pat_top();
                            if !self.eat(COMMA) {
                                break;
                            }
                        }
                        self.expect(R_PAREN);
                        self.finish();
                    }
                    Some(L_CURLY) => {
                        self.start_at(cp, RECORD_PAT);
                        self.record_pat_field_list();
                        self.finish();
                    }
                    _ => {
                        self.start_at(cp, PATH_PAT);
                        self.finish();
                    }
                }
            }
            Some(STRING | DECIMAL_NUMBER) => {
                self.err_bump("only integer and boolean literals are allowed in patterns");
            }
            Some(AMP | AMP2) => {
                self.err_bump("references are not supported");
                self.pat_atom();
            }
            _ => self.err_missing_node(
                "expected a pattern",
                &[
                    R_PAREN, R_CURLY, R_BRACK, COMMA, SEMICOLON, FAT_ARROW, COLON, EQ, PIPE,
                ],
            ),
        }
    }

    fn literal_pat(&mut self) {
        self.start(LITERAL_PAT);
        self.eat(MINUS);
        if matches!(self.current(), Some(INT_NUMBER | TRUE_KW | FALSE_KW)) {
            self.start(LITERAL);
            self.bump();
            self.finish();
        } else {
            self.error_missing("expected an integer literal");
        }
        self.finish();
    }

    fn record_pat_field_list(&mut self) {
        self.start(RECORD_PAT_FIELD_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(kind) if is_item_start(kind) => break,
                Some(DOT2) => {
                    self.start(REST_PAT);
                    self.bump();
                    self.finish();
                    self.eat(COMMA);
                    break;
                }
                Some(IDENT) => {
                    self.start(RECORD_PAT_FIELD);
                    if self.nth_at(1, COLON) {
                        self.name_ref();
                        self.bump();
                        self.pat_top();
                    } else {
                        self.start(IDENT_PAT);
                        self.name();
                        self.finish();
                    }
                    self.finish();
                }
                Some(_) => {
                    self.err_bump("expected a field");
                    continue;
                }
            }
            if !self.eat(COMMA) {
                if self.at(IDENT) || self.at(DOT2) {
                    self.error_missing("expected `,`");
                    continue;
                }
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }
}

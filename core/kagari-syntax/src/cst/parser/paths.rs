//! Names, paths and generic arguments.

use super::Parser;
use crate::cst::SyntaxKind::*;

/// Where a path appears; decides how generic arguments are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathMode {
    /// `A<B>` (and `A::<B>`).
    Type,
    /// Only the turbofish `a::<B>`.
    Expr,
    /// No generic arguments (use trees, attributes).
    Use,
}

impl Parser<'_> {
    pub(super) fn name(&mut self) {
        if self.at(IDENT) {
            self.start(NAME);
            self.bump();
            self.finish();
        } else {
            self.error_missing("expected a name");
        }
    }

    /// Wraps the next token (an `IDENT`) in a `NAME_REF`.
    pub(super) fn name_ref(&mut self) {
        self.start(NAME_REF);
        self.bump();
        self.finish();
    }

    pub(super) fn at_literal_start(&self) -> bool {
        matches!(
            self.current(),
            Some(INT_NUMBER | DECIMAL_NUMBER | STRING | TRUE_KW | FALSE_KW)
        )
    }

    /// Wraps the next token (a literal) in a `LITERAL`.
    pub(super) fn literal(&mut self) {
        self.start(LITERAL);
        self.bump();
        self.finish();
    }

    fn nth_at_segment_start(&self, n: usize) -> bool {
        matches!(
            self.nth(n),
            Some(IDENT | SELF_KW | SUPER_KW | CRATE_KW | SELF_TYPE_KW)
        )
    }

    pub(super) fn at_path_start(&self) -> bool {
        self.nth_at_segment_start(0)
    }

    /// A `::`-separated path, nested left-recursively: `PATH(PATH(a) :: b)`.
    pub(super) fn path(&mut self, mode: PathMode) {
        let cp = self.checkpoint();
        self.start(PATH);
        self.path_segment(mode);
        self.finish();
        while self.at(COLON2) && self.nth_at_segment_start(2) {
            self.start_at(cp, PATH);
            self.bump_composite(COLON2);
            self.path_segment(mode);
            self.finish();
        }
    }

    fn path_segment(&mut self, mode: PathMode) {
        self.start(PATH_SEGMENT);
        match self.current() {
            Some(IDENT) => self.name_ref(),
            Some(SELF_KW | SUPER_KW | CRATE_KW | SELF_TYPE_KW) => self.bump(),
            _ => self.error_missing("expected a path segment"),
        }
        let turbofish = self.at(COLON2) && self.nth_at(2, L_ANGLE);
        match mode {
            PathMode::Type if turbofish => self.generic_arg_list(true),
            PathMode::Type if self.at(L_ANGLE) => self.generic_arg_list(false),
            PathMode::Expr if turbofish => self.generic_arg_list(true),
            _ => {}
        }
        self.finish();
    }

    fn at_generic_arg_start(&self) -> bool {
        self.at(MINUS) || self.at(L_CURLY) || self.at_literal_start() || self.at_type_start()
    }

    /// `<…>` or `::<…>`, closed by a single `>` token.
    pub(super) fn generic_arg_list(&mut self, turbofish: bool) {
        self.start(GENERIC_ARG_LIST);
        if turbofish {
            self.bump_composite(COLON2);
        }
        self.bump();
        while self.at_generic_arg_start() {
            if self.at(MINUS) {
                self.start(CONST_ARG);
                self.start(PREFIX_EXPR);
                self.bump();
                if self.at_literal_start() {
                    self.literal();
                } else {
                    self.error_missing("expected a literal");
                }
                self.finish();
                self.finish();
            } else if self.at_literal_start() {
                self.start(CONST_ARG);
                self.literal();
                self.finish();
            } else if self.at(L_CURLY) {
                self.start(CONST_ARG);
                self.block_expr();
                self.finish();
            } else {
                self.start(TYPE_ARG);
                self.ty();
                self.finish();
            }
            if !self.list_comma(Self::at_generic_arg_start) {
                break;
            }
        }
        self.expect(R_ANGLE);
        self.finish();
    }
}

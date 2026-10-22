//! Patterns.

use super::Parser;
use super::paths::PathMode;
use crate::cst::SyntaxKind::*;

impl Parser<'_> {
    pub(super) fn at_pat_start(&self) -> bool {
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
                    | SUPER_KW
                    | CRATE_KW
                    | SELF_TYPE_KW
                    | L_PAREN
                    | MUT_KW
            )
        ) || self.at(DOT2)
    }

    fn at_single_pipe(&self) -> bool {
        self.at(PIPE) && !self.at(PIPE2)
    }

    /// A pattern with top-level alternatives (`A | B`, optional leading `|`).
    pub(super) fn pat_top(&mut self) {
        let cp = self.checkpoint();
        let leading = self.at_single_pipe();
        if leading {
            self.bump();
        }
        self.pat_single();
        if leading || self.at_single_pipe() {
            while self.at_single_pipe() {
                self.bump();
                self.pat_single();
            }
            self.start_at(cp, OR_PAT);
            self.finish();
        }
    }

    /// A pattern without top-level alternatives.
    pub(super) fn pat_single(&mut self) {
        if self.at(DOT2EQ) {
            self.start(RANGE_PAT);
            self.bump_composite(DOT2EQ);
            self.range_bound();
            self.finish();
            return;
        }
        if self.at(DOT2) {
            self.start(REST_PAT);
            self.bump_composite(DOT2);
            self.finish();
            return;
        }
        match self.current() {
            Some(UNDERSCORE) => {
                self.start(WILDCARD_PAT);
                self.bump();
                self.finish();
            }
            Some(L_PAREN) => {
                let cp = self.checkpoint();
                self.bump();
                if self.eat(R_PAREN) {
                    self.start_at(cp, TUPLE_PAT);
                    self.finish();
                    return;
                }
                self.pat_top();
                let kind = if self.at(COMMA) {
                    while self.eat(COMMA) {
                        if !self.at_pat_start() {
                            break;
                        }
                        self.pat_top();
                    }
                    TUPLE_PAT
                } else {
                    PAREN_PAT
                };
                self.expect(R_PAREN);
                self.start_at(cp, kind);
                self.finish();
            }
            Some(MINUS | INT_NUMBER | DECIMAL_NUMBER | STRING | TRUE_KW | FALSE_KW) => {
                let cp = self.checkpoint();
                self.literal_pat();
                self.maybe_range_pat(cp);
            }
            Some(MUT_KW) => {
                self.start(IDENT_PAT);
                self.bump();
                self.name();
                self.finish();
            }
            Some(IDENT)
                if !(self.nth_at(1, COLON2)
                    || self.nth_at(1, L_PAREN)
                    || self.nth_at(1, L_CURLY)
                    || self.nth_at(1, DOT2)) =>
            {
                self.start(IDENT_PAT);
                self.name();
                self.finish();
            }
            Some(IDENT | SELF_KW | SUPER_KW | CRATE_KW | SELF_TYPE_KW) => {
                let cp = self.checkpoint();
                self.path(PathMode::Expr);
                match self.current() {
                    Some(L_PAREN) => {
                        self.start_at(cp, TUPLE_STRUCT_PAT);
                        self.bump();
                        while self.at_pat_start() {
                            self.pat_top();
                            if !self.list_comma(Self::at_pat_start) {
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
                        self.maybe_range_pat(cp);
                    }
                }
            }
            _ => self.err_missing_node(
                "expected a pattern",
                &[
                    R_PAREN, R_CURLY, R_BRACK, COMMA, SEMICOLON, COLON, EQ, PIPE, IF_KW, IN_KW,
                ],
            ),
        }
    }

    /// After a literal or path pattern at `cp`: `a..=b`, `a..b` or `a..`.
    fn maybe_range_pat(&mut self, cp: rowan::Checkpoint) {
        if self.at(DOT2EQ) {
            self.start_at(cp, RANGE_PAT);
            self.bump_composite(DOT2EQ);
            self.range_bound();
            self.finish();
        } else if self.at(DOT2) {
            self.start_at(cp, RANGE_PAT);
            self.bump_composite(DOT2);
            if self.at_range_bound_start() {
                self.range_bound();
            }
            self.finish();
        }
    }

    fn at_range_bound_start(&self) -> bool {
        self.at(MINUS) || self.at_literal_start() || self.at_path_start()
    }

    fn range_bound(&mut self) {
        if self.at(MINUS) || self.at_literal_start() {
            self.literal_pat();
        } else if self.at_path_start() {
            self.start(PATH_PAT);
            self.path(PathMode::Expr);
            self.finish();
        } else {
            self.error_missing("expected a range end");
        }
    }

    fn literal_pat(&mut self) {
        self.start(LITERAL_PAT);
        self.eat(MINUS);
        if self.at_literal_start() {
            self.literal();
        } else {
            self.error_missing("expected a literal");
        }
        self.finish();
    }

    fn record_pat_field_list(&mut self) {
        self.start(RECORD_PAT_FIELD_LIST);
        self.bump();
        let at_field = |p: &Self| matches!(p.current(), Some(IDENT | MUT_KW)) || p.at(DOT2);
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(_) if self.at(DOT2) => {
                    self.start(REST_PAT);
                    self.bump_composite(DOT2);
                    self.finish();
                }
                Some(IDENT) if self.nth_at(1, COLON) && !self.nth_at(1, COLON2) => {
                    self.start(RECORD_PAT_FIELD);
                    self.name_ref();
                    self.bump();
                    self.pat_top();
                    self.finish();
                }
                Some(IDENT | MUT_KW) => {
                    self.start(RECORD_PAT_FIELD);
                    self.start(IDENT_PAT);
                    self.eat(MUT_KW);
                    self.name();
                    self.finish();
                    self.finish();
                }
                Some(_) if self.at_item_start() => break,
                Some(_) => {
                    self.err_bump("expected a field");
                    continue;
                }
            }
            if !self.list_comma(at_field) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }
}

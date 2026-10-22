//! Items, attributes, generics and use trees.

use super::Parser;
use super::paths::PathMode;
use super::types::ParamMode;
use crate::cst::SyntaxKind::{self, *};

impl Parser<'_> {
    /// Whether the next tokens start an item (also the item-level recovery set).
    pub(super) fn at_item_start(&self) -> bool {
        match self.current() {
            Some(
                FN_KW | CONST_KW | STATIC_KW | STRUCT_KW | ENUM_KW | TYPE_KW | TRAIT_KW | IMPL_KW
                | MOD_KW | USE_KW | PUB_KW | POUND,
            ) => true,
            Some(IDENT) => self.at_contextual(0, "pushdown") && self.nth_at(1, FN_KW),
            _ => false,
        }
    }

    pub(super) fn items(&mut self, nested: bool) {
        loop {
            match self.current() {
                None => break,
                Some(R_CURLY) if nested => break,
                Some(_) if self.at_item_start() => self.item(false),
                Some(_) => {
                    self.error("expected an item");
                    self.start(ERROR);
                    loop {
                        self.bump();
                        match self.current() {
                            None | Some(R_CURLY) => break,
                            Some(_) if self.at_item_start() => break,
                            Some(_) => {}
                        }
                    }
                    self.finish();
                }
            }
        }
    }

    /// An item with its attributes and visibility. Must be called at an item start, so it
    /// always consumes at least one token.
    pub(super) fn item(&mut self, assoc: bool) {
        let cp = self.checkpoint();
        let mut has_attrs = false;
        while self.at(POUND) {
            self.attr();
            has_attrs = true;
        }
        self.visibility();
        let kind = match self.current() {
            Some(FN_KW) => FN,
            Some(CONST_KW) if self.nth_at(1, FN_KW) => FN,
            Some(IDENT) if self.at_contextual(0, "pushdown") && self.nth_at(1, FN_KW) => FN,
            Some(CONST_KW) => CONST,
            Some(STATIC_KW) => STATIC,
            Some(STRUCT_KW) => STRUCT,
            Some(ENUM_KW) => ENUM,
            Some(TYPE_KW) => TYPE_ALIAS,
            Some(TRAIT_KW) => TRAIT,
            Some(IMPL_KW) => IMPL,
            Some(MOD_KW) => MOD,
            Some(USE_KW) => USE,
            _ => {
                self.start_at(cp, ERROR);
                self.error(if has_attrs {
                    "expected an item after attributes"
                } else {
                    "expected an item"
                });
                self.finish();
                return;
            }
        };
        if assoc && !matches!(kind, FN | CONST | TYPE_ALIAS) {
            self.error("expected an associated item");
        }
        self.start_at(cp, kind);
        match kind {
            FN => {
                if self.at(CONST_KW) {
                    self.bump();
                } else if self.at(IDENT) {
                    self.bump_remap(PUSHDOWN_KW);
                }
                self.bump();
                self.fn_rest();
            }
            _ => {
                self.bump();
                match kind {
                    CONST => self.const_rest(),
                    STATIC => self.static_rest(),
                    STRUCT => self.struct_rest(),
                    ENUM => self.enum_rest(),
                    TYPE_ALIAS => self.type_alias_rest(),
                    TRAIT => self.trait_rest(),
                    IMPL => self.impl_rest(),
                    MOD => self.mod_rest(),
                    _ => self.use_rest(),
                }
            }
        }
        self.finish();
    }

    fn visibility(&mut self) {
        if self.at(PUB_KW) {
            self.start(VISIBILITY);
            self.bump();
            self.finish();
        }
    }

    fn attrs(&mut self) {
        while self.at(POUND) {
            self.attr();
        }
    }

    fn attr(&mut self) {
        self.start(ATTR);
        self.bump();
        if self.at(BANG) {
            self.err_bump("inner attributes are not supported");
        }
        if self.expect(L_BRACK) {
            if self.at_path_start() {
                self.path(PathMode::Use);
            } else {
                self.error_missing("expected an attribute name");
            }
            if matches!(self.current(), Some(L_PAREN | L_BRACK | L_CURLY)) {
                self.token_tree();
            }
            self.expect(R_BRACK);
        }
        self.finish();
    }

    fn token_tree(&mut self) {
        self.start(TOKEN_TREE);
        let mut stack: Vec<SyntaxKind> = Vec::new();
        if let Some(open) = self.current() {
            stack.push(closer(open));
        }
        self.bump();
        while let Some(&expected) = stack.last() {
            match self.current() {
                None => {
                    self.error_missing(format!("expected {}", expected.describe()));
                    break;
                }
                Some(open @ (L_PAREN | L_BRACK | L_CURLY)) => {
                    stack.push(closer(open));
                    self.bump();
                }
                Some(close @ (R_PAREN | R_BRACK | R_CURLY)) => {
                    if close == expected {
                        stack.pop();
                        self.bump();
                    } else if stack.len() == 1 {
                        self.error_missing(format!("expected {}", expected.describe()));
                        break;
                    } else {
                        self.err_bump("mismatched closing delimiter");
                    }
                }
                Some(_) => self.bump(),
            }
        }
        self.finish();
    }

    fn fn_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        if self.at(L_PAREN) {
            self.param_list(ParamMode::Fn);
        } else {
            self.error_missing("expected `(`");
        }
        if self.at(THIN_ARROW) {
            self.ret_type();
        }
        if self.at(L_CURLY) {
            self.block_expr();
        } else if !self.eat(SEMICOLON) {
            self.error_missing("expected `{` or `;`");
        }
    }

    fn at_self_param(&self) -> bool {
        match self.current() {
            Some(SELF_KW) => !self.nth_at(1, COLON2),
            Some(MUT_KW) => self.nth_at(1, SELF_KW),
            Some(AMP) => {
                self.nth_at(1, SELF_KW) || (self.nth_at(1, MUT_KW) && self.nth_at(2, SELF_KW))
            }
            _ => false,
        }
    }

    fn self_param(&mut self) {
        self.start(SELF_PARAM);
        self.eat(AMP);
        self.eat(MUT_KW);
        self.bump();
        if self.at(COLON) && !self.at(COLON2) {
            self.error("explicit `self` types are not supported");
            self.start(ERROR);
            self.bump();
            self.ty();
            self.finish();
        }
        self.finish();
    }

    fn at_param_start(&self, mode: ParamMode) -> bool {
        match mode {
            ParamMode::Fn => self.at_self_param() || self.at_pat_start(),
            ParamMode::Type => self.at_type_start(),
        }
    }

    /// `( … )` parameters of a function (patterns with types) or of a function-pointer or
    /// closure type (bare types).
    pub(super) fn param_list(&mut self, mode: ParamMode) {
        self.start(PARAM_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_PAREN) => break,
                _ => {}
            }
            match mode {
                ParamMode::Type => {
                    if !self.at_type_start() {
                        break;
                    }
                    self.start(PARAM);
                    self.ty();
                    self.finish();
                }
                ParamMode::Fn => {
                    if self.at_self_param() {
                        self.self_param();
                    } else if self.at_pat_start() {
                        self.start(PARAM);
                        self.pat_single();
                        if self.at(COLON) && !self.at(COLON2) {
                            self.bump();
                            self.ty();
                        } else {
                            self.error_missing("expected `:`");
                        }
                        self.finish();
                    } else if self.at_item_start() || self.at(L_CURLY) || self.at(SEMICOLON) {
                        break;
                    } else {
                        self.err_bump("expected a parameter");
                        continue;
                    }
                }
            }
            if !self.list_comma(|p| p.at_param_start(mode)) {
                break;
            }
        }
        self.expect(R_PAREN);
        self.finish();
    }

    pub(super) fn ret_type(&mut self) {
        self.start(RET_TYPE);
        self.bump_composite(THIN_ARROW);
        self.ty();
        self.finish();
    }

    fn struct_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        match self.current() {
            Some(L_CURLY) => self.record_field_list(),
            Some(L_PAREN) => {
                self.tuple_field_list();
                self.expect(SEMICOLON);
            }
            Some(SEMICOLON) => self.bump(),
            _ => self.error_missing("expected `{`, `(` or `;`"),
        }
    }

    fn at_field_start(&self) -> bool {
        matches!(self.current(), Some(IDENT | PUB_KW | POUND))
    }

    fn record_field_list(&mut self) {
        self.start(RECORD_FIELD_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(_) if self.at_field_start() => {
                    self.start(RECORD_FIELD);
                    self.attrs();
                    self.visibility();
                    self.name();
                    if self.at(COLON) && !self.at(COLON2) {
                        self.bump();
                        self.ty();
                    } else {
                        self.error_missing("expected `:`");
                    }
                    self.finish();
                }
                Some(_) if self.at_item_start() => break,
                Some(_) => {
                    self.err_bump("expected a field");
                    continue;
                }
            }
            if !self.list_comma(Self::at_field_start) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    fn at_tuple_field_start(&self) -> bool {
        self.at_type_start() || matches!(self.current(), Some(PUB_KW | POUND))
    }

    fn tuple_field_list(&mut self) {
        self.start(TUPLE_FIELD_LIST);
        self.bump();
        while self.at_tuple_field_start() {
            self.start(TUPLE_FIELD);
            self.attrs();
            self.visibility();
            self.ty();
            self.finish();
            if !self.list_comma(Self::at_tuple_field_start) {
                break;
            }
        }
        self.expect(R_PAREN);
        self.finish();
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
        let at_variant = |p: &Self| matches!(p.current(), Some(IDENT | POUND));
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(_) if at_variant(self) => self.variant(),
                Some(_) if self.at_item_start() => break,
                Some(_) => {
                    self.err_bump("expected a variant");
                    continue;
                }
            }
            if !self.list_comma(at_variant) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    fn variant(&mut self) {
        self.start(VARIANT);
        self.attrs();
        self.name();
        match self.current() {
            Some(L_PAREN) => self.tuple_field_list(),
            Some(L_CURLY) => self.record_field_list(),
            _ => {}
        }
        if self.at(EQ) && !self.at(EQ2) && !self.at(FAT_ARROW) {
            self.error("explicit discriminants are not supported");
            self.start(ERROR);
            self.bump();
            self.expr();
            self.finish();
        }
        self.finish();
    }

    fn type_alias_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        if self.at(COLON) && !self.at(COLON2) {
            self.bump();
            self.type_bound_list();
        }
        if self.eat(EQ) {
            self.ty();
        }
        self.expect(SEMICOLON);
    }

    fn const_rest(&mut self) {
        self.name();
        if self.at(COLON) && !self.at(COLON2) {
            self.bump();
            self.ty();
        } else {
            self.error_missing("expected `:`");
        }
        if self.eat(EQ) {
            self.expr();
        }
        self.expect(SEMICOLON);
    }

    fn static_rest(&mut self) {
        self.eat(MUT_KW);
        self.name();
        if self.at(COLON) && !self.at(COLON2) {
            self.bump();
            self.ty();
        } else {
            self.error_missing("expected `:`");
        }
        if self.expect(EQ) {
            self.expr();
        }
        self.expect(SEMICOLON);
    }

    fn mod_rest(&mut self) {
        self.name();
        if self.at(L_CURLY) {
            self.start(ITEM_LIST);
            self.bump();
            self.items(true);
            self.expect(R_CURLY);
            self.finish();
        } else if !self.eat(SEMICOLON) {
            self.error_missing("expected `{` or `;`");
        }
    }

    fn impl_rest(&mut self) {
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        self.ty();
        if self.eat(FOR_KW) {
            self.ty();
        }
        self.assoc_item_list();
    }

    fn trait_rest(&mut self) {
        self.name();
        if self.at(L_ANGLE) {
            self.generic_param_list();
        }
        if self.at(COLON) && !self.at(COLON2) {
            self.bump();
            self.type_bound_list();
        }
        self.assoc_item_list();
    }

    fn assoc_item_list(&mut self) {
        if !self.at(L_CURLY) {
            self.error_missing("expected `{`");
            return;
        }
        self.start(ASSOC_ITEM_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(_) if self.at_item_start() => self.item(true),
                Some(_) => self.err_bump("expected an associated item"),
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    fn use_rest(&mut self) {
        if self.at_use_tree_start() {
            self.use_tree();
        } else {
            self.error_missing("expected a use tree");
        }
        self.expect(SEMICOLON);
    }

    fn at_use_tree_start(&self) -> bool {
        self.at(STAR) || self.at(L_CURLY) || self.at_path_start()
    }

    fn use_tree(&mut self) {
        self.start(USE_TREE);
        if self.at(STAR) {
            self.bump();
        } else if self.at(L_CURLY) {
            self.use_tree_list();
        } else {
            self.path(PathMode::Use);
            if self.at(COLON2) {
                self.bump_composite(COLON2);
                if self.at(STAR) {
                    self.bump();
                } else if self.at(L_CURLY) {
                    self.use_tree_list();
                } else {
                    self.error_missing("expected `*` or `{`");
                }
            } else if self.at(AS_KW) {
                self.start(RENAME);
                self.bump();
                if self.at(UNDERSCORE) {
                    self.bump();
                } else {
                    self.name();
                }
                self.finish();
            }
        }
        self.finish();
    }

    fn use_tree_list(&mut self) {
        self.start(USE_TREE_LIST);
        self.bump();
        loop {
            match self.current() {
                None | Some(R_CURLY) => break,
                Some(_) if self.at_use_tree_start() => self.use_tree(),
                Some(SEMICOLON) => break,
                Some(_) => {
                    self.err_bump("expected a use tree");
                    continue;
                }
            }
            if !self.list_comma(Self::at_use_tree_start) {
                break;
            }
        }
        self.expect(R_CURLY);
        self.finish();
    }

    pub(super) fn generic_param_list(&mut self) {
        self.start(GENERIC_PARAM_LIST);
        self.bump();
        let at_param = |p: &Self| matches!(p.current(), Some(IDENT | CONST_KW));
        while at_param(self) {
            if self.at(CONST_KW) {
                self.start(CONST_PARAM);
                self.bump();
                self.name();
                if self.at(COLON) && !self.at(COLON2) {
                    self.bump();
                    self.ty();
                } else {
                    self.error_missing("expected `:`");
                }
            } else {
                self.start(TYPE_PARAM);
                self.name();
                if self.at(COLON) && !self.at(COLON2) {
                    self.bump();
                    self.type_bound_list();
                }
            }
            self.finish();
            if !self.list_comma(at_param) {
                break;
            }
        }
        self.expect(R_ANGLE);
        self.finish();
    }

    fn type_bound_list(&mut self) {
        self.start(TYPE_BOUND_LIST);
        loop {
            self.start(TYPE_BOUND);
            if self.at_path_start() {
                self.start(PATH_TYPE);
                self.path(PathMode::Type);
                self.finish();
            } else {
                self.error_missing("expected a trait bound");
            }
            self.finish();
            if !self.eat(PLUS) {
                break;
            }
        }
        self.finish();
    }
}

fn closer(open: SyntaxKind) -> SyntaxKind {
    match open {
        L_BRACK => R_BRACK,
        L_CURLY => R_CURLY,
        _ => R_PAREN,
    }
}

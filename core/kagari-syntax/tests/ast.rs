//! Typed AST accessors on small inputs.

use kagari_syntax::ast::{
    self, AstNode, BinOp, CmpOp, Condition, ElseBranch, FieldName, FnKind, FnPtrKind,
    FormDelimiter, FormEntry, HasName, HasVisibility, LiteralKind, PathSegmentKind, TextItem,
};
use kagari_syntax::{SourceKind, parse};

fn first<N: AstNode<Language = kagari_syntax::KagariLanguage>>(text: &str, kind: SourceKind) -> N {
    let parse = parse(text, kind);
    assert!(parse.errors().is_empty(), "{:?}", parse.errors());
    parse
        .syntax_node()
        .descendants()
        .find_map(N::cast)
        .unwrap_or_else(|| panic!("no node in {text:?}"))
}

#[test]
fn items() {
    let f: ast::Fn = first(
        "#[scene] pub pushdown fn A(&mut self, x: u8) -> u8 { x }",
        SourceKind::Kis,
    );
    assert_eq!(f.fn_kind(), FnKind::Pushdown);
    assert_eq!(f.name().map(|n| n.text()).as_deref(), Some("A"));
    assert!(f.visibility().is_some());
    let params = f.param_list().unwrap_or_else(|| panic!("params"));
    let self_param = params.self_param().unwrap_or_else(|| panic!("self"));
    assert!(self_param.amp_token().is_some() && self_param.mut_token().is_some());
    assert_eq!(params.params().count(), 1);
    assert!(f.body().and_then(|b| b.tail_expr()).is_some());

    let imp: ast::Impl = first(
        "impl<A> Step for Choice<A> { type Output = A; }",
        SourceKind::Kis,
    );
    let text = |t: Option<ast::Type>| t.map(|t| t.syntax().text().to_string());
    assert_eq!(text(imp.trait_type()).as_deref(), Some("Step"));
    assert_eq!(text(imp.self_type()).as_deref(), Some("Choice<A>"));
    let inherent: ast::Impl = first("impl Unit {}", SourceKind::Kis);
    assert!(inherent.trait_type().is_none());
    assert_eq!(text(inherent.self_type()).as_deref(), Some("Unit"));

    let path: ast::Path = first("use crate::a::b;", SourceKind::Kis);
    let kinds: Vec<_> = path.segments().filter_map(|s| s.kind()).collect();
    assert!(matches!(kinds.first(), Some(PathSegmentKind::CrateKw)));
    assert_eq!(kinds.len(), 3);

    let ty: ast::FnPtrType = first("type F = scene fn(u8);", SourceKind::Kis);
    assert_eq!(ty.kind(), FnPtrKind::Scene);
}

#[test]
fn expressions() {
    let bin: ast::BinExpr = first("fn f() { a <= b }", SourceKind::Kis);
    assert_eq!(bin.op_kind(), Some(BinOp::Cmp(CmpOp::Le)));
    assert_eq!(
        bin.lhs().map(|e| e.syntax().text().to_string()).as_deref(),
        Some("a")
    );
    assert_eq!(
        bin.rhs().map(|e| e.syntax().text().to_string()).as_deref(),
        Some("b")
    );

    let if_expr: ast::IfExpr = first(
        "fn f() { if let Some(x) = y { 1 } else if z { 2 } }",
        SourceKind::Kis,
    );
    assert!(matches!(if_expr.condition(), Some(Condition::LetExpr(_))));
    assert!(if_expr.then_branch().is_some());
    assert!(matches!(if_expr.else_branch(), Some(ElseBranch::If(_))));

    let field: ast::FieldExpr = first("fn f() { t.0.1 }", SourceKind::Kis);
    assert_eq!(field.field(), Some(FieldName::Index(1)));

    let range: ast::RangeExpr = first("fn f() { ..=5 }", SourceKind::Kis);
    assert!(range.start().is_none() && range.end().is_some() && range.is_inclusive());

    let pat: ast::RangePat = first("fn f() { match x { 1.. => 0 } }", SourceKind::Kis);
    assert!(pat.start().is_some() && pat.end().is_none() && !pat.is_inclusive());

    let lit: ast::Literal = first("const X: u8 = 0x1Fu8;", SourceKind::Kis);
    assert_eq!(lit.kind(), Some(LiteralKind::Int));
    assert_eq!(lit.int_value(), Some((31, Some("u8".to_owned()))));

    let array: ast::ArrayExpr = first("fn f() { [0; 4] }", SourceKind::Kis);
    assert!(array.repeat().is_some());
}

#[test]
fn forms() {
    let form: ast::FormExpr = first(
        "fn f() { choice!{ id = \"a\", t![\"x\"] if c => A::B } }",
        SourceKind::Kix,
    );
    assert_eq!(form.name_text().as_deref(), Some("choice"));
    assert_eq!(form.delimiter(), Some(FormDelimiter::Brace));
    let entries: Vec<_> = form.entries().collect();
    assert!(matches!(entries.first(), Some(FormEntry::FormAttr(_))));
    let Some(FormEntry::FormArm(arm)) = entries.get(1) else {
        panic!("expected an arm");
    };
    assert!(matches!(arm.text(), Some(ast::Expr::TextForm(_))));
    assert!(arm.guard().and_then(|g| g.condition()).is_some());
    assert!(arm.value().is_some());

    let text: ast::TextForm = first(
        "fn f() { t![\"a\\n\" [\"漢字\"](\"かん\"|\"じ\") ![\"b\"](e()) {x}] }",
        SourceKind::Kix,
    );
    let items: Vec<_> = text.items().collect();
    assert_eq!(items.len(), 4);
    let Some(TextItem::TextString(s)) = items.first() else {
        panic!("expected a string");
    };
    assert_eq!(s.value().as_deref(), Some("a\n"));
    let Some(TextItem::TextRuby(ruby)) = items.get(1) else {
        panic!("expected a ruby");
    };
    assert_eq!(ruby.readings().map(|r| r.strings().count()), Some(2));
    let Some(TextItem::TextEffect(effect)) = items.get(2) else {
        panic!("expected an effect");
    };
    assert!(effect.base().is_some() && effect.effect().is_some());
}

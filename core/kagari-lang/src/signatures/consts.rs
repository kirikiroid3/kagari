//! The stage 3 type-level constant folder: array lengths, `Int` bounds and const generic
//! arguments.

use crate::db::Db;
use crate::def_map::{Def, Resolution};
use crate::diagnostic::{Anchor, Diag, DiagnosticCode, NodeId, Owner};
use crate::hir::{BinaryOp, Expr, ExprId, ExprStore, Literal, UnaryOp, body_of};
use crate::input::Program;
use crate::item_tree::{ItemId, ItemKind, item_tree};
use crate::resolve::{StoreResolution, body_resolution};
use crate::ty::Const;

const UNSUPPORTED: &str =
    "only literals, arithmetic, constants and const parameters are allowed here";

/// Folds `expr`, an expression of `store` (the signature or body `owner`, resolved by
/// `res`), to a type-level constant: integer literals, unary `-`, `+ - * / %`, blocks with only
/// a tail, const generic parameters and `const` items. Anything else is reported as
/// `E2002` and folds to `Const::Error`.
pub fn fold_const_expr(
    db: &dyn Db,
    program: Program,
    owner: Owner,
    store: &ExprStore,
    res: &StoreResolution,
    expr: ExprId,
    diags: &mut Vec<Diag>,
) -> Const {
    let mut folder = Folder {
        db,
        program,
        stack: Vec::new(),
    };
    folder.fold(owner, store, res, expr, diags)
}

/// Folds the initializer of the `const` item `item`, reporting at `anchor` when it is not a
/// type-level constant.
pub(crate) fn fold_const_item(
    db: &dyn Db,
    program: Program,
    item: ItemId,
    anchor: Anchor,
    diags: &mut Vec<Diag>,
) -> Const {
    let mut folder = Folder {
        db,
        program,
        stack: Vec::new(),
    };
    folder.fold_item(item, anchor, diags)
}

struct Folder<'a> {
    db: &'a dyn Db,
    program: Program,
    stack: Vec<ItemId>,
}

impl Folder<'_> {
    fn diag(&self, anchor: Anchor, code: DiagnosticCode, message: String) -> Diag {
        Diag::new(anchor, code, message)
    }

    fn fold(
        &mut self,
        owner: Owner,
        store: &ExprStore,
        res: &StoreResolution,
        expr: ExprId,
        diags: &mut Vec<Diag>,
    ) -> Const {
        let range = Anchor::Node(owner, NodeId::Expr(expr));
        let unsupported = |this: &Self, diags: &mut Vec<Diag>| {
            diags.push(this.diag(
                range,
                DiagnosticCode::TypeUnsupportedConst,
                UNSUPPORTED.to_owned(),
            ));
            Const::Error
        };
        match store.expr(expr) {
            Expr::Missing => Const::Error,
            Expr::Literal(Literal::Int { value, .. }) => match i128::try_from(*value) {
                Ok(value) => Const::Value(value),
                Err(_) => {
                    diags.push(self.diag(
                        range,
                        DiagnosticCode::TypeConstOverflow,
                        "integer constant is too large".to_owned(),
                    ));
                    Const::Error
                }
            },
            Expr::Unary {
                op: UnaryOp::Neg,
                expr: inner,
            } => match self.fold(owner, store, res, *inner, diags) {
                Const::Value(value) => match value.checked_neg() {
                    Some(value) => Const::Value(value),
                    None => self.overflow(range, diags),
                },
                Const::Error => Const::Error,
                Const::Param { .. } | Const::Infer(_) => unsupported(self, diags),
            },
            Expr::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
                ) =>
            {
                let lhs = self.fold(owner, store, res, *lhs, diags);
                let rhs = self.fold(owner, store, res, *rhs, diags);
                match (lhs, rhs) {
                    (Const::Value(a), Const::Value(b)) => {
                        if matches!(op, BinaryOp::Div | BinaryOp::Rem) && b == 0 {
                            diags.push(self.diag(
                                range,
                                DiagnosticCode::TypeConstOverflow,
                                "division by zero in a constant".to_owned(),
                            ));
                            return Const::Error;
                        }
                        let value = match op {
                            BinaryOp::Add => a.checked_add(b),
                            BinaryOp::Sub => a.checked_sub(b),
                            BinaryOp::Mul => a.checked_mul(b),
                            BinaryOp::Div => a.checked_div(b),
                            _ => a.checked_rem(b),
                        };
                        match value {
                            Some(value) => Const::Value(value),
                            None => self.overflow(range, diags),
                        }
                    }
                    (Const::Error, _) | (_, Const::Error) => Const::Error,
                    _ => unsupported(self, diags),
                }
            }
            Expr::Block {
                stmts,
                tail: Some(tail),
                ..
            } if stmts.is_empty() => self.fold(owner, store, res, *tail, diags),
            Expr::Path(_) => match res.exprs.get(&expr) {
                Some(Resolution::GenericParam { owner, index }) => Const::Param {
                    owner: *owner,
                    index: *index,
                },
                Some(Resolution::Def(Def::Item(item)) | Resolution::Assoc(item))
                    if self.is_const(*item) =>
                {
                    self.fold_item(*item, range, diags)
                }
                Some(Resolution::Error) | None => Const::Error,
                Some(_) => unsupported(self, diags),
            },
            _ => unsupported(self, diags),
        }
    }

    fn overflow(&self, range: Anchor, diags: &mut Vec<Diag>) -> Const {
        diags.push(self.diag(
            range,
            DiagnosticCode::TypeConstOverflow,
            "arithmetic overflow in a constant".to_owned(),
        ));
        Const::Error
    }

    fn is_const(&self, item: ItemId) -> bool {
        matches!(
            item_tree(self.db, item.file)
                .item(item.index)
                .map(|data| &data.kind),
            Some(ItemKind::Const { .. })
        )
    }

    fn fold_item(&mut self, item: ItemId, range: Anchor, diags: &mut Vec<Diag>) -> Const {
        let name = item_tree(self.db, item.file)
            .item(item.index)
            .and_then(|data| data.name.clone())
            .unwrap_or_default();
        if self.stack.contains(&item) {
            diags.push(self.diag(
                range,
                DiagnosticCode::TypeUnsupportedConst,
                format!("cycle detected when evaluating constant `{name}`"),
            ));
            return Const::Error;
        }
        let Some(body) = body_of(self.db, item) else {
            return Const::Error;
        };
        let Some(body_res) = &body_resolution(self.db, self.program, item).body else {
            return Const::Error;
        };
        self.stack.push(item);
        let mut inner = Vec::new();
        let value = self.fold(
            Owner::Body(item),
            &body.store,
            &body_res.store,
            body.value,
            &mut inner,
        );
        self.stack.pop();
        if let Some(cycle) = inner
            .iter()
            .find(|diag| diag.message.starts_with("cycle detected"))
        {
            let message = cycle.message.clone();
            diags.push(self.diag(range, DiagnosticCode::TypeUnsupportedConst, message));
            return Const::Error;
        }
        if !inner.is_empty() {
            diags.push(self.diag(
                range,
                DiagnosticCode::TypeUnsupportedConst,
                format!(
                    "constant `{name}` cannot be used here: its value must be made of literals, \
                     arithmetic, constants and const parameters"
                ),
            ));
            return Const::Error;
        }
        match value {
            Const::Param { .. } => {
                diags.push(self.diag(
                    range,
                    DiagnosticCode::TypeUnsupportedConst,
                    format!("constant `{name}` depends on generic parameters"),
                ));
                Const::Error
            }
            value => value,
        }
    }
}

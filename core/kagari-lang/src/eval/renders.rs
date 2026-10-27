//! The renders steps use: for every `Step` of a scene's MIR, the renders its record can
//! carry, each evaluated once on the probe screen (is it a keyframe?) and, when it
//! inspects its input, on the empty screen for its errors.

use rustc_hash::FxHashSet;

use crate::diagnostic::{Anchor, NodeId, Owner};
use crate::hir::{Expr, ExprId};
use crate::infer::ValueRef;
use crate::item_tree::{FnKind, ItemId};
use crate::mir::{ItemMir, Terminator, item_mir};
use crate::signatures::FnSig;
use crate::ty::{FnDefId, FnRef, GenericArgs, Ty};

use super::const_exprs::{ConstExprs, const_exprs};
use super::ctx::BodyRef;
use super::machine::screen::Screen;
use super::machine::value::{ClosureValue, Value};
use super::machine::{CallSite, ErrorKind};
use super::value::ConstValue;
use super::{Driver, RenderId, RenderInfo};

/// A candidate render of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RenderCand {
    Fn(FnRef),
    Closure(ItemId, ExprId),
}

fn push_value(value: &ConstValue, out: &mut Vec<RenderCand>) {
    match value {
        ConstValue::Fn(fn_ref) => out.push(RenderCand::Fn(fn_ref.clone())),
        ConstValue::Closure { owner, expr } => out.push(RenderCand::Closure(*owner, *expr)),
        _ => {}
    }
}

/// A story body with what render evaluation reads.
#[derive(Clone, Copy)]
struct Story<'db> {
    body: BodyRef<'db>,
    consts: &'db ConstExprs,
    mir: &'db ItemMir,
}

impl<'db> Driver<'db> {
    /// Evaluates the render of every step of a story body.
    pub(super) fn renders(&mut self, item: ItemId, game: bool, functions: &[(ItemId, &'db FnSig)]) {
        let (Some(body), Some(consts), Some(mir)) = (
            self.ctx.body(item),
            const_exprs(self.db, self.program, item),
            item_mir(self.db, self.program, item).as_ref(),
        ) else {
            return;
        };
        let story = Story { body, consts, mir };
        let mut seen = FxHashSet::default();
        for (_, block) in mir.main.blocks.iter() {
            let (Terminator::Step { .. }, Some(call)) = (&block.term, block.term_src) else {
                continue;
            };
            if seen.insert(call) {
                self.step_renders(story, game, call, functions);
            }
        }
    }

    fn step_renders(
        &mut self,
        story: Story<'db>,
        game: bool,
        call: ExprId,
        functions: &[(ItemId, &'db FnSig)],
    ) {
        let body = story.body;
        let store = &body.body.store;
        let Expr::Call { callee, args } = store.expr(call) else {
            return;
        };
        let is_push_step = matches!(
            body.infer.paths.get(callee),
            Some(ValueRef::Fn(FnRef { def: FnDefId::Item(func), .. }))
                if *func == self.ctx.std.push_step
        );
        let [step] = args.as_slice() else {
            return;
        };
        if !is_push_step {
            return;
        }
        let mut cands = Vec::new();
        let trigger = match store.expr(*step) {
            Expr::Record { fields, .. } => match fields.iter().find(|(name, _)| name == "render") {
                Some((_, render)) => {
                    if story.consts.contains(*step) {
                        // A constant record is promoted as a whole.
                        if let Some(value) =
                            self.const_expr_value(body, story.consts, story.mir, *step)
                        {
                            self.record_render(&value, &mut cands);
                        }
                    } else {
                        self.render_cands(story, *render, false, functions, &mut cands);
                    }
                    self.render_site(body, *render)
                }
                None => return,
            },
            _ if story.consts.contains(*step) => {
                if let Some(value) = self.const_expr_value(body, story.consts, story.mir, *step) {
                    self.record_render(&value, &mut cands);
                }
                *step
            }
            _ => {
                self.all_renders(functions, &mut cands);
                *step
            }
        };
        for cand in cands {
            self.render(body.item, call, trigger, game, cand);
        }
    }

    /// The render of a constant `Say` or `Choice` record.
    fn record_render(&self, value: &ConstValue, out: &mut Vec<RenderCand>) {
        let std = self.ctx.std;
        let render = match value {
            ConstValue::Adt { item: adt, .. } if *adt == std.say.item => std.say.field("render"),
            ConstValue::Adt { item: adt, .. } if *adt == std.choice.item => {
                std.choice.field("render")
            }
            _ => None,
        };
        if let Some(Some(Some(render))) = render
            .and_then(|index| value.field(index))
            .map(|render| std.as_option(render))
        {
            push_value(render, out);
        }
    }

    /// Where a step's render errors are reported: the `x` of `render: Some(x)` (what
    /// `render = x` desugars to), else the whole `render` value.
    fn render_site(&self, body: BodyRef<'db>, render: ExprId) -> ExprId {
        let std = self.ctx.std;
        match body.body.store.expr(render) {
            Expr::Call { callee, args } => match (body.infer.paths.get(callee), args.as_slice()) {
                (
                    Some(ValueRef::Fn(FnRef {
                        def: FnDefId::VariantCtor(adt, variant),
                        ..
                    })),
                    [inner],
                ) if *adt == std.option.item && std.option.variant("Some") == Some(*variant) => {
                    *inner
                }
                _ => render,
            },
            _ => render,
        }
    }

    /// The renders `expr` (an `Option<Render>`, or a `Render` when `inner`) can yield.
    fn render_cands(
        &self,
        story: Story<'db>,
        expr: ExprId,
        inner: bool,
        functions: &[(ItemId, &'db FnSig)],
        out: &mut Vec<RenderCand>,
    ) {
        let body = story.body;
        let std = self.ctx.std;
        if story.consts.contains(expr) {
            let value = self.const_expr_value(body, story.consts, story.mir, expr);
            match (inner, value) {
                (true, Some(value)) => push_value(&value, out),
                (false, Some(value)) => {
                    if let Some(Some(render)) = std.as_option(&value) {
                        push_value(render, out);
                    }
                }
                (_, None) => {}
            }
            return;
        }
        match body.body.store.expr(expr) {
            Expr::Call { callee, args } if !inner => {
                let is_some = matches!(
                    body.infer.paths.get(callee),
                    Some(ValueRef::Fn(FnRef { def: FnDefId::VariantCtor(adt, variant), .. }))
                        if *adt == std.option.item && std.option.variant("Some") == Some(*variant)
                );
                match args.as_slice() {
                    [render] if is_some => {
                        self.render_cands(story, *render, true, functions, out);
                    }
                    _ => self.all_renders(functions, out),
                }
            }
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.render_cands(story, *then_branch, inner, functions, out);
                if let Some(else_branch) = else_branch {
                    self.render_cands(story, *else_branch, inner, functions, out);
                }
            }
            Expr::Match { arms, .. } => {
                for arm in arms {
                    self.render_cands(story, arm.body, inner, functions, out);
                }
            }
            Expr::Block {
                tail: Some(tail), ..
            } => self.render_cands(story, *tail, inner, functions, out),
            Expr::Closure { .. } if inner => out.push(RenderCand::Closure(body.item, expr)),
            _ => self.all_renders(functions, out),
        }
    }

    /// Every named `const fn(Screen) -> Screen`: the values a runtime `Render` can hold.
    fn all_renders(&self, functions: &[(ItemId, &'db FnSig)], out: &mut Vec<RenderCand>) {
        let screen = Ty::adt(self.ctx.std.screen, GenericArgs::empty());
        for (item, sig) in functions {
            if sig.kind == FnKind::Const && sig.params == [screen.clone()] && sig.ret == screen {
                out.push(RenderCand::Fn(FnRef {
                    def: FnDefId::Item(*item),
                    self_ty: None,
                    args: GenericArgs::empty(),
                }));
            }
        }
    }

    /// Evaluates a render once: on the probe screen to decide whether it is a keyframe, and
    /// on `Screen::empty()` for its errors when it inspects its input.
    fn render(
        &mut self,
        item: ItemId,
        call: ExprId,
        trigger: ExprId,
        game: bool,
        cand: RenderCand,
    ) {
        let (id, callee) = match cand {
            RenderCand::Fn(fn_ref) => match fn_ref.def {
                FnDefId::Item(item) => (RenderId::Fn(item), Value::Fn(fn_ref)),
                _ => return,
            },
            RenderCand::Closure(owner, expr) => (
                RenderId::Closure(owner, expr),
                Value::Closure(Box::new(ClosureValue::plain(owner, expr))),
            ),
        };
        if self.out.renders.contains_key(&id) {
            return;
        }
        let site = || CallSite {
            at: Some(Anchor::Node(Owner::Body(item), NodeId::Expr(call))),
            ty: None,
        };
        let mut machine = self.ctx.machine();
        let probe = machine.call_value(site(), &callee, vec![Value::Probe]);
        let (keyframe, error) = match probe {
            Ok(output) => (machine.freeze(&output).is_ok(), None),
            Err(error) if error.kind == ErrorKind::Inspected => {
                let mut machine = self.ctx.machine();
                let empty = Value::Screen(std::sync::Arc::new(Screen::empty(self.ctx.std)));
                (
                    false,
                    machine.call_value(site(), &callee, vec![empty]).err(),
                )
            }
            Err(error) => (false, Some(error)),
        };
        self.out.renders.insert(id, RenderInfo { keyframe });
        if let Some(error) = error {
            let at = Anchor::Node(Owner::Body(item), NodeId::Expr(trigger));
            self.report(game, at, &error, None);
        }
    }
}

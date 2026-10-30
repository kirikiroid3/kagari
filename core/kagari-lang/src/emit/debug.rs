//! The debug information of one emitted function (`kagari_debug::FunctionDebug`), built from
//! its MIR, its HIR and its register allocation next to its code. It never changes the
//! bytecode. See `docs/dev/src/compiler/emit.md#debug-information`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use kagari_debug::{FunctionDebug, LocalDebug, RegisterRange, ScopeDebug};
use kagari_syntax::TextRange;
use sha2::{Digest, Sha256};

use crate::eval::const_exprs::children;
use crate::hir::source_map::StoreSourceMap;
use crate::hir::{Condition, Expr, ExprId, ExprStore, PatId, body_of, body_source_map_of};
use crate::mir::pretty::dump_body;
use crate::mir::{Body, LocalKind};
use crate::resolve::{LocalId, body_resolution};

use super::codegen::{LocalRange, mir_body};
use super::{Emitter, Instance, index};

impl Emitter<'_> {
    /// The debug information of `instance`, emitted as the function `path` whose code has the
    /// word `offsets`; `locals` is where its registers hold its MIR locals, by instruction.
    pub(super) fn function_debug(
        &self,
        instance: &Instance,
        path: String,
        file: Option<String>,
        locals: &[Option<LocalRange>],
        offsets: &[u32],
    ) -> FunctionDebug {
        let body = match instance {
            Instance::Ctor(_) | Instance::Builtin(_) => None,
            _ => mir_body(self, instance),
        };
        let body_hash = match body {
            Some(body) => sha256_hex(&dump_body(
                self.db,
                self.program,
                &path,
                body,
                self.root_dir.as_deref(),
            )),
            None => sha256_hex(&path),
        };
        let (scopes, locals) = match (instance, body) {
            (Instance::Fn(_) | Instance::Closure { .. } | Instance::ConstInit(..), Some(body)) => {
                self.scopes_and_locals(instance, body, locals, offsets)
            }
            _ => (Vec::new(), Vec::new()),
        };
        FunctionDebug {
            path,
            file,
            body_hash,
            scopes,
            locals,
        }
    }

    /// The lexical scopes of the body, then one local per source binding of its MIR.
    fn scopes_and_locals(
        &self,
        instance: &Instance,
        body: &Body,
        ranges: &[Option<LocalRange>],
        offsets: &[u32],
    ) -> (Vec<ScopeDebug>, Vec<LocalDebug>) {
        let owner = body.source.owner();
        let (Some(hir), Some(map)) = (body_of(self.db, owner), body_source_map_of(self.db, owner))
        else {
            return (Vec::new(), Vec::new());
        };
        let mut walk = Scopes {
            store: &hir.store,
            map,
            scopes: Vec::new(),
        };
        match instance {
            Instance::Closure { expr, .. } => walk.root(*expr, &[]),
            _ => walk.root(hir.value, &hir.params),
        }
        let scopes = walk.scopes;
        // One entry per binding: a binding lowered to several MIR locals merges their ranges.
        let mut bindings: BTreeMap<LocalId, Vec<RegisterRange>> = BTreeMap::new();
        for (local, decl) in body.locals.iter() {
            let LocalKind::User(id) = decl.kind else {
                continue;
            };
            let range = ranges
                .get(local.into_raw().into_u32() as usize)
                .copied()
                .flatten()
                .and_then(|range| in_words(range, offsets));
            bindings.entry(id).or_default().extend(range);
        }
        let resolution = body_resolution(self.db, self.program, owner).body.as_ref();
        let locals = bindings
            .into_iter()
            .map(|(id, ranges)| {
                let binding = resolution
                    .filter(|res| (id.into_raw().into_u32() as usize) < res.locals.len())
                    .map(|res| &res.locals[id]);
                let scope = binding
                    .and_then(|binding| binding.pat)
                    .and_then(|pat| map.pats.get(pat))
                    .map_or(0, |range| innermost(&scopes, *range));
                LocalDebug {
                    name: binding
                        .map_or_else(|| "?".to_owned(), |binding| binding.name.to_string()),
                    scope,
                    ranges: merge(ranges),
                }
            })
            .collect();
        (scopes, locals)
    }
}

fn sha256_hex(text: &str) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()).iter() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// A range of instructions as a range of code words; `None` when empty.
fn in_words(range: LocalRange, offsets: &[u32]) -> Option<RegisterRange> {
    let start = *offsets.get(range.start as usize)?;
    let end = *offsets.get(range.end as usize)?;
    (start < end).then_some(RegisterRange {
        start,
        end,
        register: range.reg,
        cell: range.cell,
    })
}

/// `ranges` sorted, without overlaps (a later range starts where an earlier one ends), with
/// touching ranges of one register joined.
fn merge(mut ranges: Vec<RegisterRange>) -> Vec<RegisterRange> {
    ranges.sort_by_key(|range| (range.start, range.end, range.register, range.cell));
    let mut out: Vec<RegisterRange> = Vec::with_capacity(ranges.len());
    for mut range in ranges {
        if let Some(last) = out.last_mut() {
            range.start = range.start.max(last.end);
            if range.start >= range.end {
                continue;
            }
            if range.start == last.end && range.register == last.register && range.cell == last.cell
            {
                last.end = range.end;
                continue;
            }
        }
        out.push(range);
    }
    out
}

/// The innermost scope containing `range`: scopes are in preorder, so the last one that
/// contains it; the body's scope when none does (a capture declared outside a closure).
fn innermost(scopes: &[ScopeDebug], range: TextRange) -> u32 {
    let (start, end) = (u32::from(range.start()), u32::from(range.end()));
    scopes
        .iter()
        .rposition(|scope| scope.start <= start && end <= scope.end)
        .map_or(0, index)
}

/// Collects the lexical scopes of one body in preorder.
struct Scopes<'a> {
    store: &'a ExprStore,
    map: &'a StoreSourceMap,
    scopes: Vec<ScopeDebug>,
}

impl Scopes<'_> {
    /// The body's scope (`root` and the parameter patterns), then the scopes inside it.
    fn root(&mut self, root: ExprId, params: &[PatId]) {
        let range = params
            .iter()
            .filter_map(|pat| self.map.pats.get(*pat).copied())
            .fold(self.map.exprs.get(root).copied(), |acc, range| {
                Some(acc.map_or(range, |acc| acc.cover(range)))
            })
            .unwrap_or_default();
        self.scopes.push(ScopeDebug {
            parent: None,
            start: u32::from(range.start()),
            end: u32::from(range.end()),
        });
        self.walk(root, 0, true);
    }

    /// A new scope under `parent`; `parent` itself when the range is unknown.
    fn push(&mut self, parent: u32, range: Option<TextRange>) -> u32 {
        let Some(range) = range else {
            return parent;
        };
        let id = index(self.scopes.len());
        self.scopes.push(ScopeDebug {
            parent: Some(parent),
            start: u32::from(range.start()),
            end: u32::from(range.end()),
        });
        id
    }

    /// From a binding pattern to the end of the expression it is visible in.
    fn span(&self, pat: PatId, last: ExprId) -> Option<TextRange> {
        Some(self.map.pats.get(pat)?.cover(*self.map.exprs.get(last)?))
    }

    /// The scopes inside `expr`, which is in `scope`. The root block or closure of the body
    /// is the body's scope itself; a nested closure is a function of its own.
    fn walk(&mut self, expr: ExprId, scope: u32, root: bool) {
        stacker::maybe_grow(crate::STACK_RED_ZONE, crate::STACK_GROWTH, || {
            let store = self.store;
            match store.expr(expr) {
                block @ Expr::Block { .. } => {
                    let inner = if root {
                        scope
                    } else {
                        self.push(scope, self.map.exprs.get(expr).copied())
                    };
                    for child in children(block) {
                        self.walk(child, inner, false);
                    }
                }
                Expr::Closure { body, .. } => {
                    if root {
                        self.walk(*body, scope, true);
                    }
                }
                Expr::Match { scrutinee, arms } => {
                    self.walk(*scrutinee, scope, false);
                    for arm in arms {
                        let inner = self.push(scope, self.span(arm.pat, arm.body));
                        if let Some(guard) = arm.guard {
                            self.walk(guard, inner, false);
                        }
                        self.walk(arm.body, inner, false);
                    }
                }
                Expr::If {
                    cond: Condition::Let { pat, expr: value },
                    then_branch,
                    else_branch,
                } => {
                    self.walk(*value, scope, false);
                    let inner = self.push(scope, self.span(*pat, *then_branch));
                    self.walk(*then_branch, inner, false);
                    if let Some(other) = else_branch {
                        self.walk(*other, scope, false);
                    }
                }
                Expr::While {
                    cond: Condition::Let { pat, expr: value },
                    body,
                    ..
                } => {
                    self.walk(*value, scope, false);
                    let inner = self.push(scope, self.span(*pat, *body));
                    self.walk(*body, inner, false);
                }
                Expr::For {
                    pat,
                    iterable,
                    body,
                    ..
                } => {
                    self.walk(*iterable, scope, false);
                    let inner = self.push(scope, self.span(*pat, *body));
                    self.walk(*body, inner, false);
                }
                other => {
                    for child in children(other) {
                        self.walk(child, scope, false);
                    }
                }
            }
        });
    }
}

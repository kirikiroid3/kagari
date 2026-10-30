//! A textual dump of an item's MIR, for snapshots and debugging. It prints no offsets or
//! `ExprId`s, so it is stable under edits that do not change the code.

use std::fmt::Write as _;

use crate::db::Db;
use crate::def_map::{Def, DefMaps, def_maps};
use crate::eval::display_value;
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::paths;
use crate::resolve::body_resolution;
use crate::signatures::{ProgramSignatures, display_ty, program_signatures};
use crate::ty::{Const, FnDefId, FnRef, GenericArg, GenericArgs, Ty};

use super::item_mir;
use super::{
    AggregateKind, BasicBlock, Body, BodyKind, Callee, ConstOperand, ItemMir, Local, LocalKind,
    Operand, Place, PlaceBase, Proj, Rvalue, StatementKind, Terminator,
};

/// The MIR of `item`: its main body, then its closures, then its promoted constants; empty
/// when it has none.
pub fn dump_item(db: &dyn Db, program: Program, item: ItemId) -> String {
    let Some(mir) = item_mir(db, program, item).as_ref() else {
        return String::new();
    };
    let printer = Printer {
        db,
        program,
        maps: def_maps(db, program),
        sigs: program_signatures(db, program),
        mir,
    };
    let path = paths::assoc_path(db, printer.maps, item);
    let mut out = String::new();
    printer.body(&mut out, &path, &mir.main);
    for (index, (_, body)) in mir.closures.iter().enumerate() {
        out.push('\n');
        printer.body(&mut out, &format!("{path}::{{closure#{index}}}"), body);
    }
    for (index, body) in mir.promoted.iter().enumerate() {
        out.push('\n');
        printer.body(&mut out, &format!("{path}::{{promoted#{index}}}"), body);
    }
    out
}

struct Printer<'a> {
    db: &'a dyn Db,
    program: Program,
    maps: &'a DefMaps,
    sigs: &'a ProgramSignatures,
    mir: &'a ItemMir,
}

fn local(local: Local) -> String {
    format!("_{}", local.into_raw().into_u32())
}

fn block(block: BasicBlock) -> String {
    format!("bb{}", block.into_raw().into_u32())
}

impl Printer<'_> {
    fn ty(&self, ty: &Ty) -> String {
        display_ty(self.db, self.program, ty)
    }

    fn body(&self, out: &mut String, path: &str, body: &Body) {
        let kind = match body.kind {
            BodyKind::Fn => "fn",
            BodyKind::ConstFn => "const fn",
            BodyKind::Instant => "instant",
            BodyKind::Scene => "scene",
            BodyKind::Const => "const",
        };
        let params: Vec<String> = body
            .args()
            .map(|arg| {
                let ty = body
                    .local_decl(arg)
                    .map_or_else(|| "?".to_owned(), |decl| self.ty(&decl.ty));
                format!("{}: {ty}", local(arg))
            })
            .collect();
        let ret = body
            .local_decl(Body::RETURN)
            .map_or_else(|| "?".to_owned(), |decl| self.ty(&decl.ty));
        let _ = writeln!(out, "{kind} {path}({}) -> {ret} {{", params.join(", "));
        let owner = body.source.owner();
        for (id, decl) in body.locals.iter() {
            let note = match decl.kind {
                LocalKind::Return => "return".to_owned(),
                LocalKind::Arg => "arg".to_owned(),
                LocalKind::User(user) => self.local_name(owner, user),
                LocalKind::Temp => "temp".to_owned(),
            };
            let cell = if body.needs_cell.contains(id) {
                " cell"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "    let {}{}: {}; // {note}{cell}",
                if decl.mutable { "mut " } else { "" },
                local(id),
                self.ty(&decl.ty)
            );
        }
        for (id, data) in body.blocks.iter() {
            let entry = if id == body.entry { " (entry)" } else { "" };
            let _ = writeln!(out, "    {}{entry}: {{", block(id));
            for stmt in &data.stmts {
                let line = match &stmt.kind {
                    StatementKind::Assign(place, rvalue) => {
                        format!("{} = {}", self.place(place), self.rvalue(rvalue))
                    }
                    StatementKind::Call { dest, callee, args } => format!(
                        "{} = {}({})",
                        self.place(dest),
                        self.callee(callee),
                        self.operands(args)
                    ),
                };
                let _ = writeln!(out, "        {line};");
            }
            let _ = writeln!(out, "        {};", self.terminator(&data.term));
            out.push_str("    }\n");
        }
        out.push_str("}\n");
    }

    fn local_name(&self, owner: ItemId, user: crate::resolve::LocalId) -> String {
        body_resolution(self.db, self.program, owner)
            .body
            .as_ref()
            .filter(|res| (user.into_raw().into_u32() as usize) < res.locals.len())
            .map_or_else(|| "?".to_owned(), |res| res.locals[user].name.to_string())
    }

    fn terminator(&self, term: &Terminator) -> String {
        match term {
            Terminator::Goto(target) => format!("goto -> {}", block(*target)),
            Terminator::SwitchBool {
                cond,
                then,
                other,
                short_circuit,
            } => {
                let short = short_circuit
                    .map(|result| format!(" (short-circuit {})", local(result)))
                    .unwrap_or_default();
                format!(
                    "switch_bool({}) -> [true: {}, false: {}]{short}",
                    self.operand(cond),
                    block(*then),
                    block(*other)
                )
            }
            Terminator::SwitchVariant {
                place,
                adt,
                targets,
                otherwise,
            } => {
                let mut arms: Vec<String> = targets
                    .iter()
                    .map(|(variant, target)| {
                        format!("{}: {}", self.variant_name(*adt, *variant), block(*target))
                    })
                    .collect();
                if let Some(target) = otherwise {
                    arms.push(format!("otherwise: {}", block(*target)));
                }
                format!(
                    "switch_variant({}) -> [{}]",
                    self.place(place),
                    arms.join(", ")
                )
            }
            Terminator::Return => "return".to_owned(),
            Terminator::Unreachable => "unreachable".to_owned(),
            Terminator::Step {
                arg,
                dest,
                next,
                id,
            } => format!(
                "{} = step{id}({}) -> {}",
                self.place(dest),
                self.operand(arg),
                block(*next)
            ),
            Terminator::Rng { ty, dest, next } => format!(
                "{} = rng::<{}>() -> {}",
                self.place(dest),
                self.ty(ty),
                block(*next)
            ),
            Terminator::SceneCall {
                callee,
                args,
                dest,
                next,
            } => format!(
                "{} = scene_call {}({}) -> {}",
                self.place(dest),
                self.callee(callee),
                self.operands(args),
                block(*next)
            ),
        }
    }

    fn variant_name(&self, adt: ItemId, variant: u32) -> String {
        self.sigs
            .adt(adt)
            .and_then(|def| def.variants.get(variant as usize))
            .map_or_else(|| format!("#{variant}"), |def| def.name.to_string())
    }

    fn callee(&self, callee: &Callee) -> String {
        match callee {
            Callee::Fn(fn_ref) => self.fn_ref(fn_ref),
            Callee::Builtin(builtin, fn_ref) => {
                format!("builtin[{builtin:?}] {}", self.fn_ref(fn_ref))
            }
            Callee::Value(operand) => format!("({})", self.operand(operand)),
        }
    }

    fn fn_ref(&self, fn_ref: &FnRef) -> String {
        let path = match fn_ref.def {
            FnDefId::Item(item) => paths::assoc_path(self.db, self.maps, item),
            FnDefId::StructCtor(item) => paths::item_path(self.db, self.maps, item),
            FnDefId::VariantCtor(enum_item, index) => {
                paths::def_path(self.db, self.maps, Def::Variant { enum_item, index })
            }
        };
        let path = match &fn_ref.self_ty {
            Some(self_ty) => format!("<{} as {path}>", self.ty(self_ty)),
            None => path,
        };
        format!("{path}{}", self.generic_args(&fn_ref.args))
    }

    fn generic_args(&self, args: &GenericArgs) -> String {
        if args.is_empty() {
            return String::new();
        }
        let args: Vec<String> = args
            .iter()
            .map(|arg| match arg {
                GenericArg::Type(ty) => self.ty(ty),
                GenericArg::Const(Const::Value(value)) => value.to_string(),
                GenericArg::Const(Const::Param { owner, index }) => {
                    self.const_param(*owner, *index)
                }
                GenericArg::Const(Const::Infer(_) | Const::Error) => "_".to_owned(),
            })
            .collect();
        format!("::<{}>", args.join(", "))
    }

    fn const_param(&self, owner: ItemId, index: u32) -> String {
        self.sigs
            .generics(owner)
            .and_then(|generics| generics.params.get(index as usize))
            .map_or_else(|| format!("#{index}"), |param| param.name.to_string())
    }

    fn operands(&self, operands: &[Operand]) -> String {
        let operands: Vec<String> = operands.iter().map(|op| self.operand(op)).collect();
        operands.join(", ")
    }

    fn operand(&self, operand: &Operand) -> String {
        match operand {
            Operand::Copy(place) => format!("copy {}", self.place(place)),
            Operand::Const(constant) => match constant {
                ConstOperand::Value(value) => format!("const {}", display_value(self.db, value)),
                ConstOperand::Item { item, args } => format!(
                    "const {}{}",
                    paths::assoc_path(self.db, self.maps, *item),
                    self.generic_args(args)
                ),
                ConstOperand::Param { owner, index } => {
                    format!("const {}", self.const_param(*owner, *index))
                }
                ConstOperand::Promoted(id) => format!("const {{promoted#{}}}", id.idx()),
                ConstOperand::Fn(fn_ref) => format!("fn {}", self.fn_ref(fn_ref)),
                ConstOperand::Handle(item) => {
                    format!("handle {}", paths::item_path(self.db, self.maps, *item))
                }
                ConstOperand::Static(item) => {
                    format!("static {}", paths::item_path(self.db, self.maps, *item))
                }
            },
        }
    }

    fn place(&self, place: &Place) -> String {
        let mut out = match place.base {
            PlaceBase::Local(base) => local(base),
            PlaceBase::StoryVar(item) => {
                format!("story {}", paths::item_path(self.db, self.maps, item))
            }
        };
        for proj in &place.proj {
            out = match proj {
                Proj::Deref => format!("(*{out})"),
                Proj::Field(index) => format!("{out}.{index}"),
                Proj::Index(index) => format!("{out}[{}]", local(*index)),
                Proj::Downcast(variant) => format!("({out} as #{variant})"),
            };
        }
        out
    }

    fn rvalue(&self, rvalue: &Rvalue) -> String {
        let op = |operand: &Operand| self.operand(operand);
        match rvalue {
            Rvalue::Use(operand) => op(operand),
            Rvalue::IntBin {
                op: bin,
                lhs,
                rhs,
                ty,
            } => {
                format!(
                    "IntBin::<{}>({bin:?}, {}, {})",
                    self.ty(ty),
                    op(lhs),
                    op(rhs)
                )
            }
            Rvalue::IntNeg { operand, ty } => format!("IntNeg::<{}>({})", self.ty(ty), op(operand)),
            Rvalue::IntNot { operand, ty } => format!("IntNot::<{}>({})", self.ty(ty), op(operand)),
            Rvalue::IntClamp { operand, ty } => {
                format!("IntClamp::<{}>({})", self.ty(ty), op(operand))
            }
            Rvalue::BoolNot(operand) => format!("BoolNot({})", op(operand)),
            Rvalue::BoolBin { op: bin, lhs, rhs } => {
                format!("BoolBin({bin:?}, {}, {})", op(lhs), op(rhs))
            }
            Rvalue::DecBin { op: bin, lhs, rhs } => {
                format!("DecBin({bin:?}, {}, {})", op(lhs), op(rhs))
            }
            Rvalue::DecNeg(operand) => format!("DecNeg({})", op(operand)),
            Rvalue::RatBin { op: bin, lhs, rhs } => {
                format!("RatBin({bin:?}, {}, {})", op(lhs), op(rhs))
            }
            Rvalue::RatNeg(operand) => format!("RatNeg({})", op(operand)),
            Rvalue::Cmp { op: bin, lhs, rhs } => format!("Cmp({bin:?}, {}, {})", op(lhs), op(rhs)),
            Rvalue::Aggregate { kind, ops } => {
                let ops = self.operands(ops);
                match kind {
                    AggregateKind::Tuple => format!("({ops})"),
                    AggregateKind::Array => format!("[{ops}]"),
                    AggregateKind::Adt(ctor) => {
                        let path = match ctor.variant {
                            Some(index) => paths::def_path(
                                self.db,
                                self.maps,
                                Def::Variant {
                                    enum_item: ctor.adt,
                                    index,
                                },
                            ),
                            None => paths::item_path(self.db, self.maps, ctor.adt),
                        };
                        let args = self.generic_args(&ctor.args);
                        if ops.is_empty() {
                            format!("{path}{args}")
                        } else {
                            format!("{path}{args} {{ {ops} }}")
                        }
                    }
                }
            }
            Rvalue::Repeat { operand, count } => format!("[{}; {count}]", op(operand)),
            Rvalue::Ref { mutable, place } => format!(
                "&{}{}",
                if *mutable { "mut " } else { "" },
                self.place(place)
            ),
            Rvalue::Len(place) => format!("Len({})", self.place(place)),
            Rvalue::Closure { expr, captures } => {
                let index = self
                    .mir
                    .closures
                    .iter()
                    .position(|(src, _)| src == expr)
                    .map_or_else(|| "?".to_owned(), |index| index.to_string());
                format!("{{closure#{index}}}({})", self.operands(captures))
            }
            Rvalue::Cast { operand, ty } => format!("{} as {}", op(operand), self.ty(ty)),
        }
    }
}

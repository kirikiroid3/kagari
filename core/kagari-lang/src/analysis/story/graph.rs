//! The explored state space as the persisted story graph ([`kagari_debug::StoryGraph`]):
//! values named by path the way saves store them, steps by step ID (the context's instance
//! path and the step's ordinal), choice outcomes by arm ID. Indices of values, frames,
//! story-variable sets, contexts, nodes and exits are those of [`Explored`]. The keyframe
//! flag of every render a node refers to comes from const evaluation
//! ([`crate::eval::RenderInfo::keyframe`]), whether or not the keyframe check runs.

use kagari_debug as kd;
use rustc_hash::FxHashMap;

use crate::assets;
use crate::db::Db;
use crate::def_map::{Def, DefMaps, def_maps};
use crate::diagnostic::{Diag, DiagnosticCode};
use crate::eval::{ConstValue, RenderId, RenderInfo, Text, const_eval};
use crate::input::Program;
use crate::item_tree::ItemId;
use crate::mir::{BasicBlock, LocalKind, Terminator, item_mir};
use crate::paths;
use crate::resolve::body_resolution;
use crate::signatures::{ProgramSignatures, program_signatures};
use crate::ty::{FnDefId, FnRef};

use super::explore::{Context, Explored, Node, NodeKind, Target};
use super::keyframes::is_keyframe;
use super::liveness::scene_kept;

struct Converter<'a> {
    db: &'a dyn Db,
    program: Program,
    maps: &'a DefMaps,
    sigs: &'a ProgramSignatures,
    explored: &'a Explored,
    renders: &'a FxHashMap<RenderId, RenderInfo>,
    /// The converted values: those of `explored`, then renders that were never interned.
    values: Vec<kd::Value>,
    extra: FxHashMap<ConstValue, u32>,
    /// The values of the keyframe renders nodes refer to, in the order met.
    keyframes: Vec<u32>,
}

pub(super) fn convert(
    db: &dyn Db,
    program: Program,
    scenes: &[ItemId],
    explored: &Explored,
    diagnostics: &[Diag],
) -> kd::StoryGraph {
    let mut cv = Converter {
        db,
        program,
        maps: def_maps(db, program),
        sigs: program_signatures(db, program),
        explored,
        renders: &const_eval(db, program).renders,
        values: Vec::new(),
        extra: FxHashMap::default(),
        keyframes: Vec::new(),
    };
    cv.values = explored
        .values
        .items()
        .iter()
        .map(|value| cv.value(value))
        .collect();
    let mut incomplete: Vec<kd::Incomplete> = Vec::new();
    for diag in diagnostics {
        if matches!(
            diag.code,
            DiagnosticCode::AnalysisTooManyStates | DiagnosticCode::AnalysisIncomplete
        ) {
            let reason = kd::Incomplete {
                code: diag.code.as_str().to_owned(),
                message: diag.resolve(db).message,
            };
            if !incomplete.contains(&reason) {
                incomplete.push(reason);
            }
        }
    }
    let contexts: Vec<kd::Context> = explored
        .contexts
        .iter()
        .map(|context| cv.context(scenes, context))
        .collect();
    cv.keyframes.sort_unstable();
    cv.keyframes.dedup();
    kd::StoryGraph {
        complete: explored.complete,
        incomplete,
        vars: explored
            .vars
            .iter()
            .map(|var| paths::item_path(db, cv.maps, *var))
            .collect(),
        frames: explored
            .frames
            .items()
            .iter()
            .map(|frame| frame.iter().map(|slot| slot.map(|id| id.0)).collect())
            .collect(),
        globals: explored
            .globals
            .items()
            .iter()
            .map(|globals| globals.iter().map(|id| id.0).collect())
            .collect(),
        scenes: scenes
            .iter()
            .enumerate()
            .map(|(index, item)| cv.scene(index, *item))
            .collect(),
        contexts,
        roots: explored.roots.iter().map(|root| root.0).collect(),
        values: cv.values,
        keyframes: cv.keyframes,
    }
}

fn index(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn block_index(block: BasicBlock) -> u32 {
    block.into_raw().into_u32()
}

fn target(target: Target) -> kd::Target {
    match target {
        Target::Node(node) => kd::Target::Node(node.0),
        Target::Exit(exit) => kd::Target::Exit(exit.0),
        Target::Dead => kd::Target::Dead,
    }
}

impl Converter<'_> {
    /// The scene's path and, for every block some node is at, the names of its kept locals.
    fn scene(&self, index: usize, item: ItemId) -> kd::Scene {
        let mut blocks: Vec<BasicBlock> = self
            .explored
            .contexts
            .iter()
            .filter(|context| context.scene == index)
            .flat_map(|context| context.nodes.iter().map(|node| node.block))
            .collect();
        blocks.sort_by_key(|block| block_index(*block));
        blocks.dedup();
        let mir = item_mir(self.db, self.program, item).as_ref();
        let kept = scene_kept(self.db, self.program, item).as_ref();
        let names = body_resolution(self.db, self.program, item).body.as_ref();
        let name = |local| -> String {
            let decl = mir.and_then(|mir| mir.main.local_decl(local));
            match decl.map(|decl| decl.kind) {
                Some(LocalKind::User(user)) => names
                    .filter(|res| (user.into_raw().into_u32() as usize) < res.locals.len())
                    .map(|res| res.locals[user].name.to_string())
                    .unwrap_or_default(),
                _ => String::new(),
            }
        };
        kd::Scene {
            path: paths::assoc_path(self.db, self.maps, item),
            points: blocks
                .into_iter()
                .map(|block| kd::Point {
                    block: block_index(block),
                    locals: kept
                        .and_then(|kept| kept.get(&block))
                        .map(|set| set.iter().map(name).collect())
                        .unwrap_or_default(),
                })
                .collect(),
        }
    }

    fn context(&mut self, scenes: &[ItemId], context: &Context) -> kd::Context {
        let scene = scenes.get(context.scene).copied();
        let nodes = context
            .nodes
            .iter()
            .map(|node| self.node(scene, node))
            .collect();
        kd::Context {
            scene: index(context.scene),
            path: self.fn_path(&context.fn_ref),
            args: context.args.iter().map(|id| id.0).collect(),
            globals: context.globals.0,
            entry: target(context.entry),
            nodes,
            exits: context
                .exits
                .iter()
                .map(|exit| kd::Exit {
                    ret: exit.ret.0,
                    globals: exit.globals.0,
                    outs: exit.outs.iter().map(|id| id.0).collect(),
                })
                .collect(),
            callers: context
                .callers
                .iter()
                .map(|(ctx, node)| kd::Caller {
                    context: ctx.0,
                    node: node.0,
                })
                .collect(),
        }
    }

    fn node(&mut self, scene: Option<ItemId>, node: &Node) -> kd::Node {
        let kind = match &node.kind {
            NodeKind::Step {
                render,
                choice,
                outcomes,
                succ,
                ..
            } => {
                let step = self.step(scene, node.block);
                let render = render.as_ref().map(|render| {
                    let id = self.value_id(render);
                    if is_keyframe(self.renders, Some(render)) {
                        self.keyframes.push(id);
                    }
                    id
                });
                if *choice {
                    kd::NodeKind::Choice {
                        step,
                        render,
                        arms: outcomes
                            .iter()
                            .zip(succ)
                            .map(|((arm, value), next)| {
                                let value = self.explored.values.get(*value);
                                (self.arm_id(value, *arm), target(*next))
                            })
                            .collect(),
                    }
                } else {
                    kd::NodeKind::Say {
                        step,
                        render,
                        next: succ.first().copied().map_or(kd::Target::Dead, target),
                    }
                }
            }
            NodeKind::Call { callee, succ } => kd::NodeKind::Call {
                callee: callee.0,
                exits: succ
                    .iter()
                    .map(|(exit, next)| (exit.0, target(*next)))
                    .collect(),
            },
            NodeKind::Rng { draws, succ } => kd::NodeKind::Draw {
                draws: draws
                    .iter()
                    .zip(succ)
                    .map(|(value, next)| (value.0, target(*next)))
                    .collect(),
            },
        };
        kd::Node {
            block: block_index(node.block),
            frame: node.frame.0,
            globals: node.globals.0,
            kind,
        }
    }

    /// The ordinal of the step ending `block` of the scene.
    fn step(&self, scene: Option<ItemId>, block: BasicBlock) -> u32 {
        scene
            .and_then(|item| item_mir(self.db, self.program, item).as_ref())
            .and_then(|mir| mir.main.block(block))
            .and_then(|data| match data.term {
                Terminator::Step { id, .. } => Some(id.0),
                _ => None,
            })
            .unwrap_or(u32::MAX)
    }

    /// The ID the engine gives a choice arm: the variant path of an enum (or struct) value,
    /// else `#{position in the arm list}` (`saves::trace::arm_id` in `kagari-engine`).
    fn arm_id(&self, value: Option<&ConstValue>, arm: u32) -> String {
        match value {
            Some(ConstValue::Adt { item, variant, .. }) => format!(
                "{}::{}",
                paths::item_path(self.db, self.maps, *item),
                self.variant_name(*item, *variant)
            ),
            _ => format!("#{arm}"),
        }
    }

    fn variant_name(&self, item: ItemId, variant: u32) -> String {
        self.sigs
            .adt(item)
            .and_then(|adt| adt.variants.get(variant as usize))
            .map(|variant| variant.name.to_string())
            .unwrap_or_default()
    }

    /// The path the bytecode gives a function value (see `Emitter::instance_path`).
    fn fn_path(&self, fn_ref: &FnRef) -> String {
        match fn_ref.def {
            FnDefId::Item(item) => format!(
                "{}{}",
                paths::assoc_path(self.db, self.maps, item),
                paths::generic_args_suffix(self.db, self.sigs, self.maps, &fn_ref.args)
            ),
            FnDefId::StructCtor(item) => paths::def_path(self.db, self.maps, Def::Item(item)),
            FnDefId::VariantCtor(enum_item, index) => {
                paths::def_path(self.db, self.maps, Def::Variant { enum_item, index })
            }
        }
    }

    /// The index of `value` among the graph's values, adding it when the exploration never
    /// stored it.
    fn value_id(&mut self, value: &ConstValue) -> u32 {
        if let Some(id) = self.explored.values.find(value) {
            return id.0;
        }
        if let Some(id) = self.extra.get(value) {
            return *id;
        }
        let id = index(self.values.len());
        let converted = self.value(value);
        self.values.push(converted);
        self.extra.insert(value.clone(), id);
        id
    }

    fn value(&self, value: &ConstValue) -> kd::Value {
        let list = |items: &imbl::Vector<ConstValue>| -> Vec<kd::Value> {
            items.iter().map(|item| self.value(item)).collect()
        };
        match value {
            ConstValue::Bool(b) => kd::Value::Bool(*b),
            ConstValue::Int(i) => kd::Value::Int(*i),
            ConstValue::Decimal(d) => kd::Value::Decimal {
                mantissa: d.mantissa(),
                scale: d.scale(),
            },
            ConstValue::Rat(r) => kd::Value::Rat {
                num: r.numer(),
                den: r.denom(),
            },
            ConstValue::Text(text) => kd::Value::Text(self.text(text)),
            ConstValue::String(s) => kd::Value::String(s.to_string()),
            ConstValue::Tuple(items) => kd::Value::Tuple(list(items)),
            ConstValue::Array(items) => kd::Value::Array(list(items)),
            ConstValue::Vec(items) => kd::Value::Vec(list(items)),
            ConstValue::Adt {
                item,
                variant,
                fields,
            } => kd::Value::Adt {
                ty: paths::item_path(self.db, self.maps, *item),
                variant: self.variant_name(*item, *variant),
                fields: list(fields),
            },
            ConstValue::Asset { path, .. } => {
                kd::Value::Asset(assets::bytecode_path(self.db, self.program, path))
            }
            ConstValue::Fn(fn_ref) => kd::Value::Fn(self.fn_path(fn_ref)),
            ConstValue::Closure { owner, expr } => kd::Value::Closure(format!(
                "{}::{{closure#{}}}",
                paths::assoc_path(self.db, self.maps, *owner),
                paths::closure_number(self.db, *owner, *expr)
            )),
            ConstValue::Handle { item, .. } => {
                kd::Value::Handle(paths::item_path(self.db, self.maps, *item))
            }
            ConstValue::Screen(_) => kd::Value::Screen,
        }
    }

    fn text(&self, text: &Text) -> kd::Text {
        match text {
            Text::Plain(s) => kd::Text::Plain(s.to_string()),
            Text::Ruby { base, reading } => kd::Text::Ruby {
                base: Box::new(self.text(base)),
                reading: Box::new(self.text(reading)),
            },
            Text::MonoRuby { base, readings } => kd::Text::MonoRuby {
                base: Box::new(self.text(base)),
                readings: readings.iter().map(|reading| self.text(reading)).collect(),
            },
            Text::Effect { base, effect } => kd::Text::Effect {
                base: Box::new(self.text(base)),
                effect: Box::new(self.value(effect)),
            },
            Text::Concat(items) => kd::Text::Concat(items.iter().map(|t| self.text(t)).collect()),
        }
    }
}
